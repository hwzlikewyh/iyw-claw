// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The answer to a screenshot of a window that is minimized, or whose
/// application is hidden. Such a window shows nothing to capture, and
/// whatever a capture gave would not be what it shows when it is back.
#[cfg(target_os = "macos")]
pub fn out_of_sight_capture(why: crate::computer::helper::axwin::OutOfSight) -> HelperError {
    let what = match why {
        crate::computer::helper::axwin::OutOfSight::Minimized => "The window is minimized",
        crate::computer::helper::axwin::OutOfSight::AppHidden => {
            "The window's application is hidden"
        }
    };
    HelperError::new(
        HelperErrorCode::Occluded,
        format!(
            "{what}, so there is no picture of it to take. computer_snapshot still reads it as it \
             is, and actions by ref still reach it. To see it, it has to be back on the screen: \
             computer_restore does that if it is shared with you for control; otherwise ask the \
             user to bring it back."
        ),
    )
}
