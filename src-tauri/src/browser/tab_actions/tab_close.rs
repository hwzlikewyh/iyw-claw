use serde_json::json;

use super::super::command_runner::AgentBrowserCli;
use super::super::error::{BrowserError, BrowserErrorCode};
use super::super::manager::BrowserSessionManager;
use super::super::process::{find_processes_by_exact_session, kill_tree_checked, ProcessRecord};
use super::super::records::TabTicket;
use super::super::tab_cleanup::{cleanup_tab_ref, close_target_by_id};
use super::super::tabs::TabRuntimeHandle;
use super::super::types::BrowserStateSnapshot;

mod logging;

use logging::{log_no_runtime_cleanup, log_tab_close_cleanup, log_target_retry};

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
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        let ticket = match self.begin_tab_close(tab_id).await {
            Ok(ticket) => ticket,
            Err(error) if error.code == BrowserErrorCode::BrowserTabNotFound => {
                return self.retry_stopping_tab_cleanup(tab_id).await;
            }
            Err(error) => return Err(error),
        };
        self.finish_tab_close(&ticket).await?;
        let handle = self.tabs.take(tab_id).await;
        let snapshot = self.snapshot().await;
        let result = self
            .complete_tab_close_cleanup(&ticket, handle.as_ref())
            .await;
        if result.is_err() {
            if let Some(handle) = handle {
                self.tab_cleanups.retain_handles(vec![handle], true).await;
            }
        }
        result?;
        Ok(snapshot)
    }

    async fn retry_stopping_tab_cleanup(
        &self,
        tab_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        if let Some(result) = self.tab_cleanups.retry_tab(tab_id).await {
            result?;
            return Ok(self.snapshot().await);
        }
        let Some(handle) = self.tabs.take(tab_id).await else {
            tracing::info!(
                target: "iyw_claw_browser",
                browser_tab_id = %tab_id,
                "browser tab close was already completed"
            );
            return Ok(self.snapshot().await);
        };
        let ticket = cleanup_retry_ticket(&handle);
        let result = self
            .complete_tab_close_cleanup(&ticket, Some(&handle))
            .await;
        if result.is_err() {
            self.tab_cleanups.retain_handles(vec![handle], true).await;
        }
        result?;
        Ok(self.snapshot().await)
    }

    async fn complete_tab_close_cleanup(
        &self,
        ticket: &TabTicket,
        handle: Option<&TabRuntimeHandle>,
    ) -> Result<(), BrowserError> {
        let fallback = cleanup_fallback(handle);
        let cleanup = self.cleanup_closed_tab_resources(ticket, handle);
        match tokio::time::timeout(TAB_CLOSE_CLEANUP_TIMEOUT, cleanup).await {
            Ok((target_result, session_result)) => {
                log_tab_close_cleanup(ticket, &target_result, &session_result);
                if target_result.is_ok() && session_result.is_ok() {
                    return Ok(());
                }
                self.force_closed_tab_cleanup(ticket, fallback, "cleanup_failed")
                    .await
            }
            Err(_) => {
                self.force_closed_tab_cleanup(ticket, fallback, "cleanup_timed_out")
                    .await
            }
        }
    }

    async fn cleanup_closed_tab_resources(
        &self,
        ticket: &TabTicket,
        handle: Option<&TabRuntimeHandle>,
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
        self.cleanup_or_retain_tab_handle(handle, true).await
    }

    async fn cleanup_runtime_handle(
        &self,
        handle: &TabRuntimeHandle,
    ) -> (Result<(), BrowserError>, Result<(), BrowserError>) {
        let cleanup = CleanupFallback::from(handle);
        let target_cleanup = self.close_tab_target(&cleanup);
        let session_cleanup = cleanup_tab_ref(handle, false);
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
    ) -> Result<(), BrowserError> {
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
        target_result.and(session_result)
    }
}

fn cleanup_retry_ticket(handle: &TabRuntimeHandle) -> TabTicket {
    TabTicket {
        operation_id: format!("cleanup-retry-{}", handle.session),
        tab_id: handle.tab_id.clone(),
        runtime_generation: handle.runtime_generation,
        tab_generation: 0,
        view_generation: 0,
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
    kill_tree_checked(&cleanup.daemon).await?;
    let remaining = find_processes_by_exact_session(
        cleanup.cli.executable_path(),
        &cleanup.session,
        "agent-browser-daemon",
    );
    if !remaining.is_empty() {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "An exact browser tab session process remained alive after forced cleanup",
        )
        .retryable(true));
    }
    let _ = tokio::fs::remove_file(cleanup.cli.pid_path(&cleanup.session)).await;
    let _ = tokio::fs::remove_file(cleanup.cli.target_path(&cleanup.session)).await;
    Ok(())
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
