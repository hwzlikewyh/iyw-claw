// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The modifiers held over a pointer action, as `platform`'s driver spells
/// them; none, nothing said.
pub(super) fn put_modifiers(args: &mut Value, modifiers: Modifiers, platform: Platform) {
    let names = modifiers.driver_names(platform);
    if !names.is_empty() {
        args["modifier"] = json!(names);
    }
}

/// Keys and typing sent with the window brought to the front go in as real
/// input, and combine with whatever modifier is held at that moment (see
/// `keystate`): while the person holds one — or where that cannot be told —
/// none is sent. In the background they reach the window alone, and go as
/// asked.
pub(super) fn keys_free(
    mode: ActDelivery,
    held: impl FnOnce() -> Option<Vec<&'static str>>,
) -> Result<(), HelperError> {
    if mode != ActDelivery::Foreground {
        return Ok(());
    }
    match held() {
        Some(held) if held.is_empty() => Ok(()),
        Some(held) => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            modifiers_held(&held),
        )),
        None => Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            MODIFIERS_UNKNOWN,
        )),
    }
}

/// What the agent is told where the held modifiers cannot be told.
pub(super) const MODIFIERS_UNKNOWN: &str =
    "On this desktop iyw-claw cannot tell whether the user is holding \
     down a modifier key, and keys sent with the window brought to the front would combine with \
     one — so nothing was sent, and retrying will not change that. Leave `delivery` out to send \
     it in the background, or use computer_set_value or a click on an element by ref.";

/// What the agent is told when the person is holding `held` down.
pub(super) fn modifiers_held(held: &[&str]) -> String {
    let names = match held {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!(
        "The user is holding down {names} right now. Keys sent with the window brought to the \
         front go in as real input and would have combined with what they hold, so nothing was \
         sent. Try again in a moment; if this keeps happening, ask the user to let go of {names}."
    )
}

/// The less certain of two effects: confirmed, then unverifiable, then
/// partial, then suspected no-op.
pub(super) fn weaker(a: ActEffect, b: ActEffect) -> ActEffect {
    let rank = |e: ActEffect| match e {
        ActEffect::Confirmed => 3,
        ActEffect::Unverifiable => 2,
        ActEffect::Partial => 1,
        ActEffect::SuspectedNoop => 0,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// Whether the driver's tool for `action` on `platform` takes a delivery
/// mode: all but macOS's `set_value`, which sets a value through
/// Accessibility alone.
pub(super) fn takes_delivery(action: &WindowAction, platform: Platform) -> bool {
    !(platform == Platform::Mac && matches!(action, WindowAction::SetValue { .. }))
}

pub(super) fn put_element(args: &mut Value, element: &ElementRef) {
    args["element_token"] = json!(element_token(element));
}

/// The driver's name for an element: its snapshot's id and its index in that
/// snapshot, as the driver writes them (`s0000002a:7`).
pub(super) fn element_token(element: &ElementRef) -> String {
    format!("{}:{}", element.snapshot_id, element.index)
}

pub(super) fn put_target(args: &mut Value, at: &DriverTarget) {
    match at {
        DriverTarget::Element(element) => put_element(args, element),
        DriverTarget::Point(point) => {
            args["x"] = json!(point.x);
            args["y"] = json!(point.y);
        }
    }
}

/// One driver call, delivered as `mode` says (and as `args` already asks),
/// and what it did.
pub(super) async fn one(
    driver: &DriverProc,
    tool: &str,
    args: Value,
    mode: ActDelivery,
    timeout: Duration,
) -> Result<RawAct, HelperError> {
    let result = driver.call(tool, args, timeout).await?;
    if result.is_error {
        return Err(act_error(tool, mode, &result));
    }
    action_result(tool, mode, &result)
}

/// Read the driver's closed action result: how far it can vouch for the
/// action, and the route it took. One it refused without calling it an
/// error says why by code (`error.code`), read as any refusal is.
pub(super) fn action_result(
    tool: &str,
    mode: ActDelivery,
    result: &ToolCallResult,
) -> Result<RawAct, HelperError> {
    let structured = result.structured.as_ref();
    let effect = structured
        .and_then(|s| s.get("effect"))
        .and_then(Value::as_str);
    let effect = match effect {
        Some("confirmed") => ActEffect::Confirmed,
        Some("partial") => ActEffect::Partial,
        Some("suspected_noop") => ActEffect::SuspectedNoop,
        Some("refused") => {
            return Err(match result.code() {
                Some(_) => act_error(tool, mode, result),
                None => HelperError::new(
                    HelperErrorCode::ActionFailed,
                    format!("The application refused the {tool}."),
                ),
            })
        }
        // Delivered, and nothing said what came of it.
        _ => ActEffect::Unverifiable,
    };
    let route = structured
        .and_then(|s| s.get("route"))
        .cloned()
        .and_then(|r| serde_json::from_value::<ActRoute>(r).ok());
    Ok(RawAct {
        effect,
        route,
        submitted: None,
        submit_note: None,
        element_frame: None,
        window_frame: None,
        clipboard: None,
    })
}

/// The window class Chromium gives its windows on Windows
/// (`Chrome_WidgetWin_1`), as the driver names a refused target's.
pub(super) const CHROMIUM_WINDOW_CLASS: &str = "Chrome_WidgetWin_";

/// What the agent is told when the driver would not send the input in the
/// background. A key or text is refused for the application as a whole —
/// aimed at an element by ref as much as at the window, and every time — so
/// the words say what still reaches it in the background, rather than
/// suggest a ref. On Windows the commonest such application is one built on
/// Chromium, which drops every key that does not come from the front; the
/// driver names it by its window class. Two refusals are of another kind: on
/// Windows an application's accessibility interface that did not finish a
/// click it may already have acted on (`effect: "unverifiable"`), and on
/// Linux background input that goes through `/dev/uinput`, which the
/// session cannot write. Whether the front is to be had is the person's
/// setting, which iyw-claw knows and adds to these words.
pub(super) fn background_refusal(tool: &str, result: &ToolCallResult) -> String {
    let said = |key: &str| {
        result
            .structured
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(Value::as_str)
    };
    if said("effect") == Some("unverifiable") {
        return "The application's accessibility interface did not finish that action, and it \
                may have gone through even so. Read the window (computer_snapshot or \
                computer_screenshot) before trying it again."
            .to_string();
    }
    if result.code() == Some("uinput_unavailable") || said("cause") == Some("uinput_unavailable") {
        return "This Linux desktop takes such input in the background only through \
                /dev/uinput, which iyw-claw cannot use here, so nothing was sent."
            .to_string();
    }
    if !matches!(tool, "press_key" | "type_text") {
        return "This application does not take that kind of input in the background. Try an \
                element by ref, or computer_set_value."
            .to_string();
    }
    let mut words = "This application takes no key presses or typing while it is in the \
                     background, so nothing was sent — and trying again in the background, by \
                     ref or not, will not change that. Fill a field with computer_set_value \
                     instead, and click by ref what the key would have done (a search or submit \
                     button, in place of return), or ask the user to press it."
        .to_string();
    let chromium = result
        .structured
        .as_ref()
        .and_then(|s| s.get("target_class"))
        .and_then(Value::as_str)
        .is_some_and(|class| class.starts_with(CHROMIUM_WINDOW_CLASS));
    if chromium {
        words.push_str(
            " On Windows no application built on Chromium takes keys in the background: Edge, \
             Chrome, VS Code and other Electron apps. For a web page, iyw-claw's own browser (the \
             browser_* tools) does.",
        );
    }
    words
}

/// Said when the driver holds no capture of the window to aim a point by:
/// its latest snapshot of the window has none. The helper then takes a
/// snapshot, which captures the window, and tries once more
/// ([`needs_capture`]); these words reach the agent only if that fails too.
pub(super) const NO_CAPTURE: &str =
    "The window has no capture to aim a point by just now, so nothing was \
     sent. Take a new computer_screenshot and use a point from it.";
