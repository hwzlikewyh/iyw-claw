// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl HelperChild {
    /// Whether the helper exits by itself within `grace`.
    pub(in crate::computer::local) async fn exits_within(&self, grace: Duration) -> bool {
        match self {
            #[cfg(target_os = "macos")]
            HelperChild::Mac(child) => tokio::time::timeout(grace, child.wait()).await.is_ok(),
            #[cfg(not(target_os = "macos"))]
            HelperChild::Tokio(child) => {
                let mut child = child.lock().await;
                tokio::time::timeout(grace, child.wait()).await.is_ok()
            }
        }
    }

    pub(in crate::computer::local) async fn stop(&self) {
        match self {
            #[cfg(target_os = "macos")]
            HelperChild::Mac(child) => {
                child.terminate();
                if tokio::time::timeout(Duration::from_secs(3), child.wait())
                    .await
                    .is_err()
                {
                    child.kill();
                    let _ = child.wait().await;
                }
            }
            #[cfg(not(target_os = "macos"))]
            HelperChild::Tokio(child) => {
                let mut child = child.lock().await;
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        }
    }
}

impl Connection {
    pub(in crate::computer::local) fn is_closed(&self) -> bool {
        *self.closed.borrow() || self.broken.load(Ordering::Acquire)
    }

    pub(in crate::computer::local) async fn stop(&self) {
        self.child.stop().await;
    }

    /// End the helper the way iyw-claw quitting does: its input ends, it stops
    /// its driver — one still starting included, once that has started — and
    /// exits. Stopped by signal if it has not within [`HELPER_EXIT_GRACE`].
    pub(in crate::computer::local) async fn shut_down(&self) {
        // The real writer is dropped, which closes the helper's input.
        *self.writer.lock().await = Box::new(tokio::io::sink());
        if !self.child.exits_within(HELPER_EXIT_GRACE).await {
            tracing::warn!("[computer] the helper did not exit on its own; stopping it");
            self.stop().await;
        }
    }

    /// Send `op`, let through at Stop count `stop`, and wait for its answer.
    pub(in crate::computer::local) async fn request(
        &self,
        op: HelperOp,
        stop: u64,
    ) -> Result<HelperReply, BackendError> {
        if self.is_closed() {
            return Err(BackendError::Unavailable("the helper exited".into()));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let sent = tokio::time::timeout(WRITE_TIMEOUT, async {
            let mut writer = self.writer.lock().await;
            write_frame(&mut *writer, &HelperRequest { id, op, stop }).await
        })
        .await;
        let failed = match sent {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(format!("the helper went away: {e}")),
            Err(_) => {
                // Stopped reading: stop it, which wakes every caller waiting
                // on it, and the next call starts a fresh one.
                self.broken.store(true, Ordering::Release);
                self.stop().await;
                Some("the helper stopped reading".to_string())
            }
        };
        if let Some(why) = failed {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(BackendError::Unavailable(why));
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err(BackendError::Unavailable("the helper exited".into())),
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err(BackendError::Failed("the helper did not answer".into()))
            }
        }
    }
}
