// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The permissions an action needs of the OS: every action reaches the
/// window through Accessibility, and a point is placed by capturing the
/// window again to measure it.
pub fn permissions_for(action: &WindowAction) -> &'static [OsPermission] {
    if action.point().is_some() {
        &[OsPermission::Accessibility, OsPermission::ScreenRecording]
    } else {
        &[OsPermission::Accessibility]
    }
}

/// Carry out `action` on the window, delivered as `mode` says: one driver
/// call, or two for typing that ends with return, both delivered alike.
/// `deliverable` is asked just before each call goes out — whatever must
/// still hold at the moment of delivery (nothing stopped, the same process,
/// an unlocked session) — and a call it refuses is not made; so is a key or
/// typing at the front while the person holds a modifier ([`keys_free`]).
#[derive(Clone, Copy)]
pub struct ActionCall<'a> {
    pub driver: &'a DriverProc,
    pub pid: u32,
    pub window_id: u64,
    pub mode: ActDelivery,
    pub deliverable: &'a Delivery,
    pub paste_ok: bool,
}

/// 每次驱动调用前重新核对授权、停止代次和输入状态。
pub async fn act(call: &ActionCall<'_>, action: &WindowAction) -> Result<RawAct, HelperError> {
    let mut args = json!({"pid": call.pid, "window_id": call.window_id});
    if takes_delivery(action, Platform::current()) {
        args["delivery_mode"] = json!(call.mode.as_str());
    }
    match action {
        WindowAction::Click { .. } => perform_click(call, action, args).await,
        WindowAction::Drag { .. } => perform_drag(call, action, args).await,
        WindowAction::Scroll { .. } => perform_scroll(call, action, args).await,
        WindowAction::Type { .. } => perform_type(call, action, args).await,
        WindowAction::Key { .. } => perform_key(call, action, args).await,
        WindowAction::SetValue { .. } => perform_set_value(call, action, args).await,
        WindowAction::InvokeMenu { .. } => perform_invoke_menu(call, action, args).await,
        WindowAction::SetFrame { .. } => perform_set_frame(call, action, args).await,
        WindowAction::Restore => perform_restore(call, action, args).await,
    }
}

pub(super) fn wrong_action() -> HelperError {
    HelperError::new(
        HelperErrorCode::BadRequest,
        "Action dispatch does not match its request",
    )
}
