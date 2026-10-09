// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A click, a drag or a scroll on the screen at points in its picture's own
/// pixels — `scale` of them to a desktop unit, as the picture was taken —
/// as real input at the front, refused where any point would land on what
/// the rules do not allow. `ready` is asked just before it goes.
pub async fn act(
    driver: &DriverProc,
    rules: &ScreenRules,
    action: &WindowAction,
    geometry: ScreenGeometry,
    ready: impl Fn() -> Result<(), HelperError>,
) -> Result<RawAct, HelperError> {
    let points = action.points();
    let scale = geometry.scale;
    if points.is_empty() || !(scale.is_finite() && scale > 0.0) {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            "an action on the screen is at a point of its picture",
        ));
    }
    if !same_screen(&geometry) {
        return Err(HelperError::new(
            HelperErrorCode::StaleRef,
            "The screen changed since that screenshot — its size or its scaling — so a point \
             read off it would not land where it was read, and nothing was sent. Take a new \
             computer_screenshot of d1 and use a point from it.",
        ));
    }
    let landings: Vec<Landing> = points.iter().map(|p| landing(p, scale)).collect();
    if landings.iter().any(|l| in_a_corner(l.screen.0, l.screen.1)) {
        return Err(HelperError::new(
            HelperErrorCode::OutOfTarget,
            "That point is in a corner of the screen, where the pointer arriving sets off what \
             the user set the corner to do — showing every window, locking the screen — so \
             nothing was sent. Pick a point away from the corners.",
        ));
    }
    let windows = windows(rules).await?;
    if landings.iter().any(|l| !lands_allowed(&windows, l.screen)) {
        return Err(HelperError::new(
            HelperErrorCode::OutOfTarget,
            "That point is on a part of the screen that is never shared — a window of iyw-claw's \
             own, of an application on the user's never-share list, or of the system's that \
             no application owns — which sharing the screen does not reach, so nothing was \
             sent. Take a new screenshot: those parts are painted over in it.",
        ));
    }
    let button = |button: &PointerButton| match button {
        PointerButton::Left => "left",
        PointerButton::Right => "right",
        PointerButton::Middle => "middle",
    };
    let (tool, args, timeout) = match action {
        WindowAction::Click {
            at: crate::computer::protocol::DriverTarget::Point(p),
            button: b,
            count,
            modifiers,
        } => {
            let (x, y) = landing(p, scale).driver;
            let mut args = json!({
                "scope": "desktop",
                "x": x,
                "y": y,
                "button": button(b),
                "count": count,
            });
            let names = modifiers.driver_names(crate::computer::keys::Platform::current());
            if !names.is_empty() {
                args["modifier"] = json!(names);
            }
            ("click", args, ACT_TIMEOUT)
        }
        WindowAction::Drag {
            from,
            to,
            button: b,
            modifiers,
            duration_ms,
        } => {
            let (from_x, from_y) = landing(from, scale).driver;
            let (to_x, to_y) = landing(to, scale).driver;
            let mut args = json!({
                "scope": "desktop",
                "from_x": from_x,
                "from_y": from_y,
                "to_x": to_x,
                "to_y": to_y,
                "button": button(b),
                "duration_ms": duration_ms,
            });
            let names = modifiers.driver_names(crate::computer::keys::Platform::current());
            if !names.is_empty() {
                args["modifier"] = json!(names);
            }
            let timeout = ACT_TIMEOUT + Duration::from_millis(u64::from(*duration_ms));
            ("drag", args, timeout)
        }
        WindowAction::Scroll {
            at: Some(crate::computer::protocol::DriverTarget::Point(p)),
            direction,
            amount,
            unit,
        } => (
            "scroll",
            json!({
                "scope": "desktop",
                "x": landing(p, scale).driver.0,
                "y": landing(p, scale).driver.1,
                "direction": match direction {
                    ScrollDirection::Up => "up",
                    ScrollDirection::Down => "down",
                    ScrollDirection::Left => "left",
                    ScrollDirection::Right => "right",
                },
                "by": match unit {
                    ScrollUnit::Line => "line",
                    ScrollUnit::Page => "page",
                },
                "amount": (*amount).clamp(1, crate::computer::types::MAX_SCROLL_AMOUNT),
            }),
            ACT_TIMEOUT,
        ),
        _ => {
            return Err(HelperError::new(
                HelperErrorCode::BadRequest,
                "the screen takes a click, a drag or a scroll at a point",
            ))
        }
    };
    ready()?;
    let result = driver.call(tool, args, timeout).await?;
    if result.is_error {
        return Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            crate::computer::helper::ops::tool_error(tool, &result).message,
        ));
    }
    Ok(RawAct {
        effect: ActEffect::Unverifiable,
        route: None,
        submitted: None,
        submit_note: None,
        element_frame: None,
        window_frame: None,
        clipboard: None,
    })
}
