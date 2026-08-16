use super::super::super::error::BrowserError;
use super::super::super::records::TabTicket;
use super::{CleanupFallback, TARGET_CLOSE_ATTEMPTS};

pub(super) fn log_target_retry(cleanup: &CleanupFallback, attempt: usize, error: &BrowserError) {
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

pub(super) fn log_no_runtime_cleanup(ticket: &TabTicket) {
    tracing::info!(
        target: "iyw_claw_browser",
        browser_tab_id = %ticket.tab_id,
        operation_id = %ticket.operation_id,
        "browser tab close needed no runtime cleanup"
    );
}

pub(super) fn log_tab_close_cleanup(
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
