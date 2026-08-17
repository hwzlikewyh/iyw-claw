use std::path::Path;

use futures_util::future::join_all;

use super::super::error::{BrowserError, BrowserErrorCode};
use super::super::manager::BrowserSessionManager;
use super::super::process::kill_tree_checked;
use super::super::tabs::TabRuntimeHandle;

impl BrowserSessionManager {
    pub(in crate::browser) async fn shutdown_tabs(&self) -> Result<(), BrowserError> {
        self.shutdown_tabs_offline("runtime teardown").await
    }

    pub(in crate::browser) async fn finish_shutdown_tabs_after_runtime(
        &self,
    ) -> Result<(), BrowserError> {
        self.shutdown_tabs_offline("runtime stopped cleanup retry")
            .await
    }

    async fn shutdown_tabs_offline(&self, reason: &'static str) -> Result<(), BrowserError> {
        let handles = self.tabs.drain().await;
        if handles.is_empty() {
            return Ok(());
        }
        tracing::info!(
            target: "iyw_claw_browser",
            reason,
            tab_count = handles.len(),
            "browser tab offline shutdown started"
        );
        let mut failed = Vec::new();
        let mut first_error = None;
        for (handle, result) in join_all(handles.into_iter().map(force_shutdown_tab_owned)).await {
            if let Err(error) = result {
                first_error.get_or_insert(error);
                failed.push(handle);
            }
        }
        self.tabs.restore_for_cleanup(failed).await;
        first_error.map_or(Ok(()), Err)
    }
}

async fn force_shutdown_tab_owned(
    handle: TabRuntimeHandle,
) -> (TabRuntimeHandle, Result<(), BrowserError>) {
    let result = force_shutdown_tab(&handle).await;
    (handle, result)
}

async fn force_shutdown_tab(handle: &TabRuntimeHandle) -> Result<(), BrowserError> {
    if let Err(error) = kill_tree_checked(&handle.daemon).await {
        tracing::error!(
            target: "iyw_claw_browser",
            browser_tab_id = %handle.tab_id,
            runtime_generation = handle.runtime_generation,
            cleanup_stage = "process",
            error_code = ?error.code,
            "browser tab process remained after offline shutdown"
        );
        return Err(error);
    }
    if let Err(error) = remove_session_files(handle).await {
        tracing::error!(
            target: "iyw_claw_browser",
            browser_tab_id = %handle.tab_id,
            runtime_generation = handle.runtime_generation,
            cleanup_stage = "session_files",
            error_code = ?error.code,
            "browser tab session files remained after offline shutdown"
        );
        return Err(error);
    }
    Ok(())
}

async fn remove_session_files(handle: &TabRuntimeHandle) -> Result<(), BrowserError> {
    remove_file_if_present(&handle.cli.pid_path(&handle.session)).await?;
    remove_file_if_present(&handle.cli.target_path(&handle.session)).await
}

async fn remove_file_if_present(path: &Path) -> Result<(), BrowserError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "A browser session file could not be removed",
        )),
    }
}
