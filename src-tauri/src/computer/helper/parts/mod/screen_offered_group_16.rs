// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The entire screen is offered on macOS and Windows; Linux has no one list
/// of every window on it to judge them by (and Wayland no picture of it).
pub(super) fn screen_offered() -> Result<(), HelperError> {
    if cfg!(any(target_os = "macos", windows)) {
        Ok(())
    } else {
        Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            "The entire screen is not offered on Linux: share windows or applications instead.",
        ))
    }
}

/// What a request cut off by the person's Stop is answered.
pub(super) fn stopped() -> HelperError {
    HelperError::new(
        HelperErrorCode::Stopped,
        "The user pressed Stop in iyw-claw's Computer use panel.",
    )
}
