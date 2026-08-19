use super::super::error::{BrowserError, BrowserErrorCode};
use super::RecoveryAttempt;

pub(super) fn log_recovery_result(
    context: RecoveryAttempt<'_>,
    result: &Result<(), BrowserError>,
) -> bool {
    match result {
        Ok(()) => tracing::info!(
            target: "iyw_claw_browser",
            browser_tab_id = %context.tab_id,
            runtime_generation = context.runtime_generation,
            attempt = context.number,
            "browser tab session recovered"
        ),
        Err(error) => tracing::warn!(
            target: "iyw_claw_browser",
            browser_tab_id = %context.tab_id,
            runtime_generation = context.runtime_generation,
            attempt = context.number,
            error_code = ?error.code,
            "browser tab session recovery failed"
        ),
    }
    match result {
        Ok(()) => true,
        Err(error) => !error.retryable,
    }
}

pub(super) fn runtime_changed() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime changed during tab recovery",
    )
    .retryable(true)
}

pub(super) fn recovery_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The recovered browser tab could not be registered",
    )
}
