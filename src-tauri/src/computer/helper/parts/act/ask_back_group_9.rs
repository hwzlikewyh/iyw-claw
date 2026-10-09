// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Ask for the window back: shown again where it was if it is minimized,
/// without being made the active window. `false` when it was not minimized.
#[cfg(windows)]
pub(super) async fn ask_back(
    _driver: &DriverProc,
    pid: u32,
    window_id: u64,
    _mode: ActDelivery,
    deliverable: &Delivery,
) -> Result<bool, HelperError> {
    use crate::computer::helper::hwnd::Restore;
    let ready = deliverable.clone();
    let asked = tokio::task::spawn_blocking(move || {
        crate::computer::helper::hwnd::restore(window_id, pid, move || ready.check())
    })
    .await
    .map_err(|e| HelperError::failed(format!("restore: {e}")))??;
    match asked {
        Restore::Asked => Ok(true),
        Restore::AlreadyShown => Ok(false),
        Restore::NotTheWindow => Err(HelperError::new(
            HelperErrorCode::NoSuchWindow,
            "the window is gone",
        )),
    }
}
