use super::error::{BrowserError, BrowserErrorCode};

pub(super) fn unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser event observer is unavailable",
    )
    .retryable(true)
}

pub(super) fn timeout() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserOperationTimeout,
        "The browser event command timed out",
    )
    .retryable(true)
}

pub(super) fn command_rejected() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser rejected an event command",
    )
}
