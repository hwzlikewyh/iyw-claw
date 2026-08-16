use futures_util::future::join_all;

use super::super::error::BrowserError;
use super::super::manager::BrowserSessionManager;
use super::super::process::kill_tree_checked;
use super::super::tab_launch::cleanup_tab_ref;
use super::super::tabs::TabRuntimeHandle;

const TAB_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

impl BrowserSessionManager {
    pub(super) async fn shutdown_tabs(&self) -> Result<(), BrowserError> {
        let handles = self.tabs.drain().await;
        let cleanup = join_all(handles.iter().map(|handle| cleanup_tab_ref(handle, true)));
        let fallback_reason = match tokio::time::timeout(TAB_SHUTDOWN_TIMEOUT, cleanup).await {
            Ok(results) if results.iter().all(Result::is_ok) => return Ok(()),
            Ok(_) => "cleanup failed",
            Err(_) => "cleanup timed out",
        };
        let result = force_shutdown_tabs(&handles, fallback_reason).await;
        if result.is_err() {
            self.tabs.restore_for_cleanup(handles).await;
        }
        result
    }
}

async fn force_shutdown_tabs(
    handles: &[TabRuntimeHandle],
    reason: &'static str,
) -> Result<(), BrowserError> {
    tracing::warn!(
        target: "iyw_claw_browser",
        reason,
        "browser tabs required forced shutdown"
    );
    join_all(handles.iter().map(force_shutdown_tab))
        .await
        .into_iter()
        .find_map(Result::err)
        .map_or(Ok(()), Err)
}

async fn force_shutdown_tab(handle: &TabRuntimeHandle) -> Result<(), BrowserError> {
    kill_tree_checked(&handle.daemon).await?;
    let _ = tokio::fs::remove_file(handle.cli.pid_path(&handle.session)).await;
    let _ = tokio::fs::remove_file(handle.cli.target_path(&handle.session)).await;
    Ok(())
}
