use serde_json::json;

use super::super::command_runner::AgentBrowserCli;
use super::super::error::{BrowserError, BrowserErrorCode};
use super::super::manager::BrowserSessionManager;
use super::super::process::{kill_tree_checked, ProcessRecord};
use super::super::records::TabTicket;
use super::super::tab_launch::{cleanup_tab, close_target_by_id};
use super::super::tabs::TabRuntimeHandle;
use super::super::types::BrowserStateSnapshot;

const TAB_CLOSE_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TARGET_CLOSE_ATTEMPTS: usize = 3;
const TARGET_CLOSE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Clone)]
struct CleanupFallback {
    tab_id: String,
    cli: AgentBrowserCli,
    session: String,
    controller_session: String,
    target_id: String,
    daemon: ProcessRecord,
}

impl BrowserSessionManager {
    pub async fn close_browser_tab(
        &self,
        tab_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let ticket = match self.begin_tab_close(tab_id).await {
            Ok(ticket) => ticket,
            Err(error) if error.code == BrowserErrorCode::BrowserTabNotFound => {
                tracing::info!(
                    target: "iyw_claw_browser",
                    browser_tab_id = %tab_id,
                    "browser tab close was already completed"
                );
                return Ok(self.snapshot().await);
            }
            Err(error) => return Err(error),
        };
        self.finish_tab_close(&ticket).await?;
        let handle = self.tabs.take(tab_id).await;
        let snapshot = self.snapshot().await;
        self.spawn_tab_close_cleanup(ticket, handle);
        Ok(snapshot)
    }

    fn spawn_tab_close_cleanup(&self, ticket: TabTicket, handle: Option<TabRuntimeHandle>) {
        let manager = self.clone();
        tokio::spawn(async move {
            let fallback = cleanup_fallback(handle.as_ref());
            let cleanup = manager.cleanup_closed_tab_resources(&ticket, handle);
            match tokio::time::timeout(TAB_CLOSE_CLEANUP_TIMEOUT, cleanup).await {
                Ok((target_result, session_result)) => {
                    log_tab_close_cleanup(&ticket, &target_result, &session_result);
                    if target_result.is_err() || session_result.is_err() {
                        manager
                            .force_closed_tab_cleanup(&ticket, fallback, "cleanup_failed")
                            .await;
                    }
                }
                Err(_) => {
                    manager
                        .force_closed_tab_cleanup(&ticket, fallback, "cleanup_timed_out")
                        .await;
                }
            }
        });
    }

    async fn cleanup_closed_tab_resources(
        &self,
        ticket: &TabTicket,
        handle: Option<TabRuntimeHandle>,
    ) -> (Result<(), BrowserError>, Result<(), BrowserError>) {
        self.streams.close_tab(&ticket.tab_id).await;
        let Some(handle) = handle else {
            log_no_runtime_cleanup(ticket);
            return (Ok(()), Ok(()));
        };
        self.cleanup_runtime_handle(handle).await
    }

    pub(in crate::browser) async fn cleanup_watcher_owned_tab(
        &self,
        handle: TabRuntimeHandle,
    ) -> Result<(), BrowserError> {
        let (target_result, session_result) = self.cleanup_runtime_handle(handle).await;
        target_result.and(session_result)
    }

    async fn cleanup_runtime_handle(
        &self,
        handle: TabRuntimeHandle,
    ) -> (Result<(), BrowserError>, Result<(), BrowserError>) {
        let cleanup = CleanupFallback::from(&handle);
        let target_cleanup = self.close_tab_target(&cleanup);
        let session_cleanup = cleanup_tab(handle, false);
        tokio::join!(target_cleanup, session_cleanup)
    }

    async fn close_tab_target(&self, cleanup: &CleanupFallback) -> Result<(), BrowserError> {
        let response = self
            .cdp_call(
                "Target.closeTarget",
                json!({ "targetId": cleanup.target_id }),
                None,
            )
            .await;
        let accepted = response.as_ref().ok().is_some_and(|value| {
            value.get("success").and_then(serde_json::Value::as_bool) != Some(false)
        });
        if accepted {
            return Ok(());
        }
        retry_target_close(
            cleanup,
            response.err().unwrap_or_else(|| {
                BrowserError::new(
                    BrowserErrorCode::BrowserRuntimeUnavailable,
                    "The browser target rejected the close request",
                )
                .retryable(true)
            }),
        )
        .await
    }

    async fn force_closed_tab_cleanup(
        &self,
        ticket: &TabTicket,
        fallback: Option<CleanupFallback>,
        reason: &'static str,
    ) {
        let (target_result, session_result) = force_cleanup(fallback).await;
        tracing::error!(
            target: "iyw_claw_browser",
            browser_tab_id = %ticket.tab_id,
            operation_id = %ticket.operation_id,
            runtime_generation = ticket.runtime_generation,
            timeout_ms = TAB_CLOSE_CLEANUP_TIMEOUT.as_millis() as u64,
            cleanup_reason = reason,
            target_error = target_result.as_ref().err().map(|error| error.message.as_str()),
            session_error = session_result.as_ref().err().map(|error| error.message.as_str()),
            "browser tab cleanup required forced fallback"
        );
    }
}

fn cleanup_fallback(handle: Option<&TabRuntimeHandle>) -> Option<CleanupFallback> {
    handle.map(CleanupFallback::from)
}

async fn force_cleanup(
    fallback: Option<CleanupFallback>,
) -> (Result<(), BrowserError>, Result<(), BrowserError>) {
    let Some(cleanup) = fallback else {
        return (Ok(()), Ok(()));
    };
    tokio::join!(
        retry_target_close(&cleanup, fallback_target_error()),
        force_session_cleanup(&cleanup)
    )
}

async fn force_session_cleanup(cleanup: &CleanupFallback) -> Result<(), BrowserError> {
    let result = kill_tree_checked(&cleanup.daemon).await;
    let _ = tokio::fs::remove_file(cleanup.cli.pid_path(&cleanup.session)).await;
    let _ = tokio::fs::remove_file(cleanup.cli.target_path(&cleanup.session)).await;
    result
}

async fn retry_target_close(
    cleanup: &CleanupFallback,
    mut last_error: BrowserError,
) -> Result<(), BrowserError> {
    for attempt in 1..=TARGET_CLOSE_ATTEMPTS {
        match close_target_by_id(
            &cleanup.cli,
            &cleanup.controller_session,
            &cleanup.target_id,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = error;
                log_target_retry(cleanup, attempt, &last_error);
            }
        }
        if attempt < TARGET_CLOSE_ATTEMPTS {
            tokio::time::sleep(TARGET_CLOSE_RETRY_DELAY).await;
        }
    }
    Err(last_error)
}

impl From<&TabRuntimeHandle> for CleanupFallback {
    fn from(handle: &TabRuntimeHandle) -> Self {
        Self {
            tab_id: handle.tab_id.clone(),
            cli: handle.cli.clone(),
            session: handle.session.clone(),
            controller_session: handle.controller_session.clone(),
            target_id: handle.target_id.clone(),
            daemon: handle.daemon.clone(),
        }
    }
}

fn fallback_target_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser target cleanup timed out",
    )
    .retryable(true)
}

fn log_target_retry(cleanup: &CleanupFallback, attempt: usize, error: &BrowserError) {
    tracing::warn!(
        target: "iyw_claw_browser",
        browser_tab_id = %cleanup.tab_id,
        target_id = %cleanup.target_id,
        attempt,
        max_attempts = TARGET_CLOSE_ATTEMPTS,
        error_code = ?error.code,
        error_message = %error.message,
        "browser target close retry failed"
    );
}

fn log_no_runtime_cleanup(ticket: &TabTicket) {
    tracing::info!(
        target: "iyw_claw_browser",
        browser_tab_id = %ticket.tab_id,
        operation_id = %ticket.operation_id,
        "browser tab close needed no runtime cleanup"
    );
}

fn log_tab_close_cleanup(
    ticket: &TabTicket,
    target_result: &Result<(), BrowserError>,
    session_result: &Result<(), BrowserError>,
) {
    log_cleanup_error(ticket, "target", target_result.as_ref().err());
    log_cleanup_error(ticket, "daemon", session_result.as_ref().err());
    if target_result.is_ok() && session_result.is_ok() {
        tracing::info!(
            target: "iyw_claw_browser",
            browser_tab_id = %ticket.tab_id,
            operation_id = %ticket.operation_id,
            runtime_generation = ticket.runtime_generation,
            "browser tab resources closed"
        );
    }
}

fn log_cleanup_error(ticket: &TabTicket, stage: &str, error: Option<&BrowserError>) {
    let Some(error) = error else { return };
    tracing::warn!(
        target: "iyw_claw_browser",
        browser_tab_id = %ticket.tab_id,
        operation_id = %ticket.operation_id,
        runtime_generation = ticket.runtime_generation,
        cleanup_stage = stage,
        error_code = ?error.code,
        error_message = %error.message,
        retryable = error.retryable,
        "browser tab resource cleanup failed after logical removal"
    );
}
