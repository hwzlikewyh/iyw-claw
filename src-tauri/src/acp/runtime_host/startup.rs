use std::sync::Arc;
use std::time::Duration;

use crate::acp::runtime_host_registry::startup::HostStartupEntry;
use tokio_util::sync::CancellationToken;

use super::HostReady;

pub(super) struct HostStartupGuard {
    shutdown: Option<CancellationToken>,
    startup: Option<Arc<HostStartupEntry>>,
}

impl HostStartupGuard {
    pub(super) fn new(shutdown: CancellationToken, startup: Arc<HostStartupEntry>) -> Self {
        Self {
            shutdown: Some(shutdown),
            startup: Some(startup),
        }
    }

    pub(super) fn into_host(mut self) -> Arc<HostStartupEntry> {
        self.shutdown.take();
        self.startup
            .take()
            .expect("startup driver entry is present")
    }

    pub(super) async fn cancel_and_reap(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.cancel();
        }
        if let Some(startup) = self.startup.as_ref().map(Arc::clone) {
            let _ = startup.abort_and_reap().await;
            self.startup.take();
        }
    }
}

impl Drop for HostStartupGuard {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.cancel();
        }
        if let Some(startup) = self.startup.take() {
            startup.abort_and_reap_in_background();
        }
    }
}

pub(super) async fn await_host_ready(
    ready: tokio::sync::oneshot::Receiver<Result<HostReady, crate::acp::error::AcpError>>,
    timeout: Duration,
    startup_cancel: CancellationToken,
) -> Result<HostReady, crate::acp::error::AcpError> {
    tokio::select! {
        result = tokio::time::timeout(timeout, ready) => match result {
            Ok(Ok(ready)) => ready,
            Ok(Err(_)) => Err(crate::acp::error::AcpError::protocol(
                "ACP runtime Host exited before initialization",
            )),
            Err(_) => Err(crate::acp::error::AcpError::InitializeTimeout),
        },
        _ = startup_cancel.cancelled() => Err(crate::acp::error::AcpError::protocol(
            "ACP runtime Host startup cancelled during shutdown",
        )),
    }
}
