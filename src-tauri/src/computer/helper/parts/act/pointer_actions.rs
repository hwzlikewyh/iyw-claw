use super::*;

pub(super) async fn perform_click(
    call: &ActionCall<'_>,
    action: &WindowAction,
    mut args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        mode,
        deliverable,
        ..
    } = *call;
    let platform = Platform::current();
    let WindowAction::Click {
        at,
        button,
        count,
        modifiers,
    } = action
    else {
        return Err(wrong_action());
    };

    let tool = match (button, count) {
        (PointerButton::Left, 1) | (PointerButton::Middle, 1) => "click",
        (PointerButton::Left, 2) => "double_click",
        (PointerButton::Right, 1) => "right_click",
        _ => {
            return Err(HelperError::new(
                HelperErrorCode::BadRequest,
                "a click is one or two presses of the left button, or one of another",
            ))
        }
    };
    if *button == PointerButton::Middle {
        args["button"] = json!("middle");
    }
    put_modifiers(&mut args, *modifiers, platform);
    put_target(&mut args, at);
    deliverable.check()?;
    one(driver, tool, args, mode, ACT_TIMEOUT).await
}

pub(super) async fn perform_drag(
    call: &ActionCall<'_>,
    action: &WindowAction,
    mut args: Value,
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
    let WindowAction::Drag {
        from,
        to,
        button,
        modifiers,
        duration_ms,
    } = action
    else {
        return Err(wrong_action());
    };

    // Whole pixels: Linux's driver rounds a drag's points where it
    // truncates a click's, and rounding up could put a point in the
    // image's last half pixel just past its edge.
    args["from_x"] = json!(from.x.floor());
    args["from_y"] = json!(from.y.floor());
    args["to_x"] = json!(to.x.floor());
    args["to_y"] = json!(to.y.floor());
    args["button"] = json!(match button {
        PointerButton::Left => "left",
        PointerButton::Right => "right",
        PointerButton::Middle => "middle",
    });
    args["duration_ms"] = json!(duration_ms);
    args["steps"] = json!((duration_ms / DRAG_STEP_MS).clamp(1, MAX_DRAG_STEPS));
    put_modifiers(&mut args, *modifiers, platform);
    #[cfg(target_os = "macos")]
    if mode == ActDelivery::Foreground {
        deliverable.check()?;
        front_of_its_app(driver, pid, window_id).await?;
    }
    deliverable.check()?;
    let timeout = ACT_TIMEOUT + Duration::from_millis(u64::from(*duration_ms));
    one(driver, "drag", args, mode, timeout).await
}

pub(super) async fn perform_scroll(
    call: &ActionCall<'_>,
    action: &WindowAction,
    mut args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        mode,
        deliverable,
        ..
    } = *call;
    let WindowAction::Scroll {
        at,
        direction,
        amount,
        unit,
    } = action
    else {
        return Err(wrong_action());
    };

    args["direction"] = json!(match direction {
        ScrollDirection::Up => "up",
        ScrollDirection::Down => "down",
        ScrollDirection::Left => "left",
        ScrollDirection::Right => "right",
    });
    args["amount"] = json!((*amount).clamp(1, crate::computer::types::MAX_SCROLL_AMOUNT));
    args["by"] = json!(match unit {
        ScrollUnit::Line => "line",
        ScrollUnit::Page => "page",
    });
    if let Some(at) = at {
        put_target(&mut args, at);
    }
    deliverable.check()?;
    one(driver, "scroll", args, mode, ACT_TIMEOUT).await
}
