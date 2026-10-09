// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Serve requests until iyw-claw closes its end. Returns the exit code.
///
/// `guard`, when there is one, is asked before each request is acted on;
/// a request from anyone but the iyw-claw that was checked ends the session.
pub async fn serve(
    mut reader: Box<dyn AsyncRead + Send + Unpin>,
    mut writer: Box<dyn AsyncWrite + Send + Unpin>,
    peer: PeerCheck,
    guard: Option<PeerGuard>,
) -> i32 {
    let (tx, mut rx) = mpsc::unbounded_channel::<HelperMessage>();
    let writer_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let bytes = encode(&message);
            let len = (bytes.len() as u32).to_le_bytes();
            if writer.write_all(&len).await.is_err()
                || writer.write_all(&bytes).await.is_err()
                || writer.flush().await.is_err()
            {
                break;
            }
        }
    });

    let _ = tx.send(HelperMessage::Ready(HelperReady {
        protocol: PROTOCOL_VERSION,
        version: env!("CARGO_PKG_VERSION").to_string(),
        peer,
        source: Some(SOURCE_FINGERPRINT.to_string()),
    }));

    let state = Arc::new(HelperState {
        driver_path: Mutex::new(None),
        driver: Mutex::new(None),
        apps: Mutex::new(AppCache::default()),
        snapshots: std::sync::Mutex::new(SnapshotBook::default()),
        stopped: Arc::new(AtomicU64::new(0)),
        permissions: Mutex::new(None),
        driver_saw: std::sync::Mutex::new(None),
        driver_stale: AtomicBool::new(false),
    });
    let mut code = EXIT_OK;
    loop {
        let request: HelperRequest = match read_frame(&mut reader).await {
            Ok(request) => request,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                // A frame that does not parse is not a request from the iyw-claw
                // this helper was built with; stop rather than guess.
                tracing::error!("unreadable request: {e}");
                code = EXIT_FAILED;
                break;
            }
        };
        if guard.as_ref().is_some_and(|g| !g.still_peer()) {
            tracing::error!(
                "a request came from a process other than the iyw-claw that was checked"
            );
            code = EXIT_PEER_REFUSED;
            break;
        }
        // A Stop takes hold the moment its frame is read, not when its task
        // is scheduled: whatever iyw-claw let through before it meets it at the
        // next check it makes before a driver call — even one whose frame
        // is read later — and whatever iyw-claw let through after it does not.
        if let HelperOp::Halt { stop } = request.op {
            state.stopped.fetch_max(stop, Ordering::AcqRel);
        }
        let state = state.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let reply = match handle(&state, request.op, request.stop).await {
                Ok(value) => HelperReply {
                    id: request.id,
                    ok: Some(value),
                    error: None,
                },
                Err(error) => HelperReply::error(request.id, error),
            };
            let _ = tx.send(HelperMessage::Reply(reply));
        });
    }
    // iyw-claw is gone (or refused): stop the driver, and do not wait on the
    // requests still in flight — nobody is left to answer. As for a Stop,
    // nothing still in flight reaches a driver, and a driver still starting
    // stops itself once it has started.
    state.stopped.store(STOP_ALL, Ordering::Release);
    if tokio::time::timeout(SHUTDOWN_GRACE, state.shutdown())
        .await
        .is_err()
    {
        tracing::warn!("the driver did not stop in time; leaving it to its closed stdin");
    }
    writer_task.abort();
    code
}
