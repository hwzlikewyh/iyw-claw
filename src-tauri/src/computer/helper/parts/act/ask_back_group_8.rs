// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Ask for the window back, through Accessibility: its application shown
/// again if hidden, then the window out of the Dock if minimized. `false`
/// when it was neither — nothing was asked. The window is looked for in the
/// driver's listing first: showing a hidden application again shows all its
/// windows, which is done only for a window of it that is still there.
#[cfg(target_os = "macos")]
pub(super) async fn ask_back(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
    _mode: ActDelivery,
    deliverable: &Delivery,
) -> Result<bool, HelperError> {
    use crate::computer::helper::axwin::Restore;
    let window = listed(driver, pid, window_id).await?;
    if window.get("is_on_screen").and_then(Value::as_bool) == Some(true) {
        return Ok(false);
    }
    let ready = deliverable.clone();
    match crate::computer::helper::axwin::restore(pid, window_id, move || ready.check()).await? {
        Restore::Asked => Ok(true),
        Restore::AlreadyShown => Ok(false),
        Restore::Unlisted => Err(HelperError::new(
            HelperErrorCode::Occluded,
            "The window cannot be reached to restore it: it may be on another desktop \
             (Space). Ask the user to bring it back.",
        )),
        Restore::Failed(code) => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            format!(
                "The window's application did not restore it (Accessibility error {code}). \
                 Ask the user to restore it."
            ),
        )),
    }
}
