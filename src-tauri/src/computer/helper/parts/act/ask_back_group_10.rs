// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Ask for the window back through the driver, which on Linux can only do it
/// by bringing it to the front — which iyw-claw asked for only where the person
/// allows it. `false` when it is on the screen already. Where the window
/// manager says the window is not minimized — on another desktop — it is not
/// brought over, as on the other platforms; where nothing says (Wayland), an
/// off-screen window is brought back.
#[cfg(not(any(target_os = "macos", windows)))]
pub(super) async fn ask_back(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
    mode: ActDelivery,
    deliverable: &Delivery,
) -> Result<bool, HelperError> {
    let window = listed(driver, pid, window_id).await?;
    if window.get("is_on_screen").and_then(Value::as_bool) == Some(true) {
        return Ok(false);
    }
    if minimized_on_x11(window_id).await == Some(false) {
        return Err(HelperError::new(
            HelperErrorCode::Occluded,
            "The window is not minimized: it is on another desktop, or otherwise out of reach. \
             Ask the user to bring it back.",
        ));
    }
    if mode != ActDelivery::Foreground {
        return Err(HelperError::new(
            HelperErrorCode::BackgroundUnavailable,
            "On Linux a window comes back on the screen only by being brought to the front.",
        ));
    }
    deliverable.check()?;
    let result = driver
        .call(
            "bring_to_front",
            json!({ "pid": pid, "window_id": window_id }),
            ACT_TIMEOUT,
        )
        .await?;
    if result.is_error {
        return Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            format!(
                "The window could not be brought back: {}. Ask the user to restore it.",
                crate::computer::helper::ops::tool_error("bring_to_front", &result).message
            ),
        ));
    }
    Ok(true)
}

/// Whether the X11 window manager has window `window_id` minimized; `None`
/// where it cannot be asked.
#[cfg(not(any(target_os = "macos", windows)))]
pub(super) async fn minimized_on_x11(window_id: u64) -> Option<bool> {
    #[cfg(all(target_os = "linux", feature = "computer-helper"))]
    {
        tokio::task::spawn_blocking(move || {
            crate::computer::helper::x11win::window_states(&[window_id])
                .get(&window_id)
                .map(|state| state.minimized)
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(not(all(target_os = "linux", feature = "computer-helper")))]
    {
        let _ = window_id;
        None
    }
}
