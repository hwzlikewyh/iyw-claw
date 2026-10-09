// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) type Io = (
    Box<dyn AsyncRead + Send + Unpin>,
    Box<dyn AsyncWrite + Send + Unpin>,
    Box<dyn AsyncRead + Send + Unpin>,
);

/// Start the helper at `path`, wait for its first frame, check who sent it,
/// and start reading its replies.
pub(super) async fn launch(path: &std::path::Path) -> Result<Arc<Connection>, BackendError> {
    let (child, (mut reader, writer, stderr), peer_fd) = spawn_helper(path)?;
    tokio::spawn(forward_stderr(stderr));

    let first =
        tokio::time::timeout(READY_TIMEOUT, read_frame::<_, HelperMessage>(&mut reader)).await;
    let ready = match first {
        Ok(Ok(HelperMessage::Ready(ready))) => ready,
        Ok(Ok(_)) => return Err(abandon(child, "the helper did not introduce itself").await),
        Ok(Err(e)) => {
            // Almost always the helper refusing its peer and exiting, which it
            // does without a word on the socket; its stderr says why.
            return Err(abandon(child, &format!("the helper closed the channel: {e}")).await);
        }
        Err(_) => return Err(abandon(child, NOT_STARTED).await),
    };
    if ready.protocol != PROTOCOL_VERSION {
        return Err(abandon(
            child,
            &format!(
                "the helper speaks protocol {}, this iyw-claw {PROTOCOL_VERSION} — reinstall iyw-claw",
                ready.protocol
            ),
        )
        .await);
    }
    if ready.version != env!("CARGO_PKG_VERSION") {
        return Err(abandon(
            child,
            "Computer helper version does not match iyw-claw; repair the installation",
        )
        .await);
    }
    if let Some(why) = stale_development_helper(ready.source.as_deref()) {
        return Err(abandon(child, why).await);
    }
    let verified = match check_helper(peer_fd, ready.peer) {
        Ok(verified) => verified,
        Err(why) => return Err(abandon(child, &why).await),
    };

    let pending: Pending = Arc::new(StdMutex::new(HashMap::new()));
    let (closed_tx, closed_rx) = watch::channel(false);
    let reader_pending = pending.clone();
    tokio::spawn(async move {
        loop {
            let frame = read_frame::<_, HelperMessage>(&mut reader).await;
            // Whoever wrote to the helper's end last must still be the helper
            // that was checked. Anyone else holding that end — a process the
            // helper was never meant to share it with — is a forger.
            if let Some(verified) = &verified {
                if !verified.still_peer() {
                    tracing::error!(
                        "[computer] a process other than the checked helper wrote to its \
                         socket; dropping the connection"
                    );
                    break;
                }
            }
            match frame {
                Ok(HelperMessage::Reply(reply)) => {
                    let tx = reader_pending
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&reply.id);
                    if let Some(tx) = tx {
                        let _ = tx.send(reply);
                    }
                }
                Ok(HelperMessage::Ready(_)) => {}
                Err(_) => break,
            }
        }
        let _ = closed_tx.send(true);
        // Dropping the senders wakes every waiting caller with "exited".
        reader_pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    });
    Ok(Arc::new(Connection {
        writer: Mutex::new(writer),
        pending,
        next_id: AtomicU64::new(1),
        closed: closed_rx,
        broken: AtomicBool::new(false),
        peer: ready.peer,
        child,
    }))
}

/// Why a development iyw-claw will not use a helper built from other sources
/// than its own — which is what `pnpm tauri dev` leaves running after an edit:
/// it builds the helper once, as it starts (never, under
/// `IYW_CLAW_SKIP_SIDECAR=1`), and only iyw-claw after that, and a stale helper
/// answers with code that is no longer there. The words name the step that
/// rebuilds it. A release iyw-claw ships with its own helper and does not ask.
pub(super) fn stale_development_helper(source: Option<&str>) -> Option<&'static str> {
    (cfg!(debug_assertions) && source != Some(SOURCE_FINGERPRINT)).then_some(
        "this development build's helper was built from other sources — run \
         `pnpm tauri:prepare-sidecars --debug` (which `pnpm tauri dev` skips under \
         IYW_CLAW_SKIP_SIDECAR=1), then restart `pnpm tauri dev`",
    )
}

pub(super) async fn abandon(child: HelperChild, why: &str) -> BackendError {
    child.stop().await;
    tracing::error!("[computer] {why}");
    BackendError::Unavailable(why.to_string())
}

/// The socket descriptor iyw-claw keeps, for asking the kernel who is on the
/// other end. `None` off macOS.
pub(super) type PeerFd = Option<i32>;
