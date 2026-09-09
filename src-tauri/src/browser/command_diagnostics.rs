use std::time::Duration;

use super::{BrowserError, BrowserErrorCode};

pub(super) fn operation_args<'a>(args: &'a [&'a str]) -> &'a [&'a str] {
    match args {
        ["--cdp", _, "--pin-tab", rest @ ..] => rest,
        _ => args,
    }
}

pub(super) fn operation_name<'a>(args: &'a [&'a str]) -> &'a str {
    operation_args(args).first().copied().unwrap_or("unknown")
}

pub(super) fn may_change_page(args: &[&str]) -> bool {
    !matches!(
        operation_name(args),
        "snapshot" | "read" | "get" | "is" | "wait"
    )
}

pub(super) fn timeout_error(args: &[&str], timeout: Duration) -> BrowserError {
    let may_have_effect = may_change_page(args);
    BrowserError::new(
        BrowserErrorCode::BrowserOperationTimeout,
        format!(
            "The browser controller timed out during {} after {} ms. Inspect the current page before retrying.",
            operation_name(args), timeout.as_millis(),
        ),
    )
    .retryable(!may_have_effect)
    .effect_may_have_occurred(may_have_effect)
}

pub(super) fn annotate_timeout(mut error: BrowserError, args: &[&str]) -> BrowserError {
    if error.code == BrowserErrorCode::BrowserOperationTimeout {
        error.message = format!("{} (operation: {})", error.message, operation_name(args));
        error.effect_may_have_occurred |= may_change_page(args);
        error.retryable &= !error.effect_may_have_occurred;
    }
    error
}
