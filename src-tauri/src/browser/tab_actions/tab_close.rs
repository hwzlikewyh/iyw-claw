use serde_json::json;

use super::super::command_runner::AgentBrowserCli;
use super::super::error::{BrowserError, BrowserErrorCode};
use super::super::manager::BrowserSessionManager;
use super::super::process::{kill_tree_checked, ProcessRecord};
use super::super::records::TabTicket;
use super::super::tab_launch::cleanup_tab;
use super::super::tabs::TabRuntimeHandle;
use super::super::types::BrowserStateSnapshot;

const TAB_CLOSE_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

type CleanupFallback = (AgentBrowserCli, String, ProcessRecord);

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
            if tokio::time::timeout(TAB_CLOSE_CLEANUP_TIMEOUT, cleanup)
                .await
                .is_err()
            {
                manager.force_closed_tab_cleanup(&ticket, fallback).await;
            }
        });
    }

    async fn cleanup_closed_tab_resources(
        &self,
        ticket: &TabTicket,
        handle: Option<TabRuntimeHandle>,
    ) {
        self.streams.close_tab(&ticket.tab_id).await;
        let Some(handle) = handle else {
            log_no_runtime_cleanup(ticket);
            return;
        };
        let target_id = handle.target_id.clone();
        let target_cleanup = self.close_tab_target(target_id);
        let session_cleanup = cleanup_tab(handle, false);
        let (target_result, session_result) = tokio::join!(target_cleanup, session_cleanup);
        log_tab_close_cleanup(ticket, &target_result, &session_result);
    }

    async fn close_tab_target(&self, target_id: String) -> Result<serde_json::Value, BrowserError> {
        let response = self
            .cdp_call("Target.closeTarget", json!({ "targetId": target_id }), None)
            .await?;
        if response.get("success").and_then(serde_json::Value::as_bool) == Some(false) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserRuntimeUnavailable,
                "The browser target rejected the close request",
            )
            .retryable(true));
        }
        Ok(response)
    }

    async fn force_closed_tab_cleanup(
        &self,
        ticket: &TabTicket,
        fallback: Option<CleanupFallback>,
    ) {
        let kill_result = force_cleanup(fallback).await;
        tracing::error!(
            target: "iyw_claw_browser",
            browser_tab_id = %ticket.tab_id,
            operation_id = %ticket.operation_id,
            runtime_generation = ticket.runtime_generation,
            timeout_ms = TAB_CLOSE_CLEANUP_TIMEOUT.as_millis() as u64,
            fallback_error = kill_result.as_ref().err().map(|error| error.message.as_str()),
            "browser tab cleanup exceeded its shutdown budget"
        );
    }
}

fn cleanup_fallback(handle: Option<&TabRuntimeHandle>) -> Option<CleanupFallback> {
    handle.map(|handle| {
        (
            handle.cli.clone(),
            handle.session.clone(),
            handle.daemon.clone(),
        )
    })
}

async fn force_cleanup(fallback: Option<CleanupFallback>) -> Result<(), BrowserError> {
    let Some((cli, session, daemon)) = fallback else {
        return Ok(());
    };
    let result = kill_tree_checked(&daemon).await;
    let _ = tokio::fs::remove_file(cli.pid_path(&session)).await;
    let _ = tokio::fs::remove_file(cli.target_path(&session)).await;
    result
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
    target_result: &Result<serde_json::Value, BrowserError>,
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
