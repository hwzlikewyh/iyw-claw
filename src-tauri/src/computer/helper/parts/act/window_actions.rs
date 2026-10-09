use super::*;

pub(super) async fn perform_invoke_menu(
    call: &ActionCall<'_>,
    action: &WindowAction,
    _args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        pid,
        window_id,
        mode,
        deliverable,
        ..
    } = *call;
    let platform = Platform::current();
    let WindowAction::InvokeMenu { path } = action else {
        return Err(wrong_action());
    };

    // The driver brings the application forward for it itself, and
    // reads nothing but these three.
    if platform == Platform::Windows {
        return Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            "Menus cannot be chosen by title on Windows: click the menu by its ref \
                     instead.",
        ));
    }
    #[cfg(target_os = "macos")]
    menu_in_reach(pid, path, call.paste_ok).await?;
    let args = json!({ "pid": pid, "window_id": window_id, "path": path });
    deliverable.check()?;
    one(driver, "invoke_menu", args, mode, ACT_TIMEOUT).await
}

pub(super) async fn perform_set_frame(
    call: &ActionCall<'_>,
    action: &WindowAction,
    _args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        pid,
        window_id,
        mode,
        deliverable,
        ..
    } = *call;
    let WindowAction::SetFrame {
        x,
        y,
        width,
        height,
    } = action
    else {
        return Err(wrong_action());
    };

    // What is not given stays as the window is now — read just
    // before, not taken from a listing an earlier move or the person
    // has overtaken.
    let now = current_frame(call).await?;
    let frame = Rect {
        x: x.unwrap_or(now.x),
        y: y.unwrap_or(now.y),
        width: width.unwrap_or(now.width),
        height: height.unwrap_or(now.height),
    };
    // The driver takes the frame alone: no delivery mode — moving a
    // window does not bring it forward.
    let args = json!({
        "pid": pid,
        "window_id": window_id,
        "x": frame.x,
        "y": frame.y,
        "width": frame.width,
        "height": frame.height,
    });
    deliverable.check()?;
    one(driver, "set_window_frame", args, mode, ACT_TIMEOUT).await
}

async fn current_frame(call: &ActionCall<'_>) -> Result<Rect, HelperError> {
    listed(call.driver, call.pid, call.window_id)
        .await?
        .get("bounds")
        .and_then(crate::computer::helper::ops::rect)
        .ok_or_else(|| {
            HelperError::new(
                HelperErrorCode::ActionFailed,
                "The window's frame could not be read, so it was not moved.",
            )
        })
}

pub(super) async fn perform_restore(
    call: &ActionCall<'_>,
    action: &WindowAction,
    _args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        pid,
        window_id,
        mode,
        deliverable,
        ..
    } = *call;
    let WindowAction::Restore = action else {
        return Err(wrong_action());
    };

    deliverable.check()?;
    restore(driver, pid, window_id, mode, deliverable).await
}
