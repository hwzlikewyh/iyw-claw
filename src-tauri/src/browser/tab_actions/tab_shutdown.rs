use futures_util::future::join_all;

use super::super::error::{BrowserError, BrowserErrorCode};
use super::super::manager::BrowserSessionManager;
use super::super::tab_cleanup::cleanup_pending_owner;

const TAB_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

impl BrowserSessionManager {
    pub(in crate::browser) async fn shutdown_tabs(&self) -> Result<(), BrowserError> {
        let _operation = self.tab_cleanups.lock_operation().await;
        let handles = self.tabs.drain().await;
        self.tab_cleanups.retain_handles(handles, true).await;
        let mut owners = self.tab_cleanups.drain().await;
        if owners.is_empty() {
            return Ok(());
        }
        let cleanup = join_all(owners.iter_mut().map(cleanup_pending_owner));
        let results = match tokio::time::timeout(TAB_SHUTDOWN_TIMEOUT, cleanup).await {
            Ok(results) => results,
            Err(_) => {
                self.tab_cleanups.restore(owners).await;
                return Err(shutdown_timeout());
            }
        };
        let mut failures = Vec::new();
        let mut first_error = None;
        for (owner, result) in owners.into_iter().zip(results) {
            if let Err(error) = result {
                first_error.get_or_insert_with(|| error.clone());
                failures.push(owner);
            }
        }
        self.tab_cleanups.restore(failures).await;
        first_error.map_or(Ok(()), Err)
    }
}

fn shutdown_timeout() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserOperationTimeout,
        "Browser tab cleanup timed out during shutdown",
    )
    .retryable(true)
}
