// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A window whose application runs with more rights than the driver: no
/// input reaches it, whichever way it is sent.
pub(super) const HIGHER_RIGHTS: &str =
    "That window's application runs with more rights than iyw-claw (as \
     administrator), and Windows lets no input from iyw-claw reach it, in the background or at the \
     front. Nothing was sent; ask the user to do this step.";

/// A call with the window brought to the front that failed before any input
/// went out.
pub(super) const FRONT_NOT_HAD: &str =
    "The window could not be brought to the front just now, so nothing \
     was sent. Try again in a moment; if it keeps failing, ask the user to do this step.";

/// A call with the window brought to the front that failed with its input
/// sent, or perhaps sent.
pub(super) const FRONT_LOST: &str =
    "This action, with the window brought to the front, did not finish \
     cleanly, so whether it went through cannot be told — it may have. Read the window \
     (computer_snapshot or computer_screenshot) before doing it again: repeating it blindly could \
     do it twice.";

/// What the driver says, word for word, only of a front it failed to have
/// before any input went out: on Windows its `foreground_unavailable: …`
/// texts that end "no input was sent" (or never got as far as the mouse); on
/// macOS the activation that came before the keys or the click. A failure
/// said any other way may have come after.
pub(super) const NOTHING_SENT: [&str; 7] = [
    "no input was sent",
    "no mouse input was sent",
    "before mouse input could be sent",
    "foreground HID delivery is unavailable",
    "could not resolve target window for foreground HID delivery",
    "rejected foreground HID activation",
    "did not become focused for foreground HID delivery",
];

/// What to say of a call with the window brought to the front that the
/// driver could not deliver; `None` for any other failure. On Windows the
/// driver says it in words, not codes (`foreground_unavailable: …` when the
/// window was not, or did not stay, at the front; `UIPI: …` for an
/// application running with more rights than it); on macOS by the code
/// `delivery_failed`, or `foreground_unavailable` for a click at a point; on
/// Linux by `foreground_unavailable` or, for a window manager that did not
/// answer in time, `foreground_timeout`. Only a failure the driver says came
/// before any input went out is one to simply try again: a click it found the
/// window gone from the front *after* may well have landed.
pub(super) fn front_failure(code: &str, text: &str) -> Option<&'static str> {
    let text = text.trim_start();
    if text.starts_with("UIPI") {
        Some(HIGHER_RIGHTS)
    } else if matches!(
        code,
        "delivery_failed" | "foreground_unavailable" | "foreground_timeout"
    ) || text.starts_with("foreground_unavailable")
        || text.starts_with("foreground_timeout")
    {
        if NOTHING_SENT.iter().any(|said| text.contains(said)) {
            Some(FRONT_NOT_HAD)
        } else {
            Some(FRONT_LOST)
        }
    } else {
        None
    }
}

/// A refused action, by the driver's code, in words for the agent. Only where
/// the driver's own text is the useful part (an action that was tried and
/// failed) is it passed on, shortened. `mode` is how the call was delivered:
/// the front fails in ways of its own.
pub(super) fn act_error(tool: &str, mode: ActDelivery, result: &ToolCallResult) -> HelperError {
    let code = result.code().unwrap_or("");
    if mode == ActDelivery::Foreground {
        if let Some(words) = front_failure(code, &result.text()) {
            return HelperError::new(HelperErrorCode::ActionFailed, words);
        }
    }
    if let Some(error) = fixed_action_error(code) {
        return error;
    }
    match code {
        "background_unavailable"
        | "background_occluded"
        | "background_pointer_failed"
        | "uinput_unavailable"
        | "SCREEN_SHARING_REQUIRES_FOREGROUND_HID" => HelperError::new(
            HelperErrorCode::BackgroundUnavailable,
            &background_refusal(tool, result),
        ),
        "type_text_synthesis_budget_exceeded" => text_budget_error(result),
        "invalid_arguments" => invalid_action_error(tool, code),
        "screen_recording_permission_denied" => {
            HelperError::permission_missing(OsPermission::ScreenRecording)
        }
        "permission_denied" | "accessibility_permission_denied" | "tcc_permission_denied" => {
            HelperError::permission_missing(OsPermission::Accessibility)
        }
        _ => unknown_action_error(tool, result),
    }
}

fn text_budget_error(result: &ToolCallResult) -> HelperError {
    let chunk = result
        .structured
        .as_ref()
        .and_then(|s| s.get("max_chunk_chars"))
        .and_then(Value::as_u64);
    let note = match chunk {
 Some(n) => format!("That is more text than can be typed in one call here; nothing was typed. Send at most {n} characters at a time."),
 None => "That is more text than can be typed in one call here; nothing was typed. Send it in smaller pieces.".to_string(),
 };
    HelperError::new(HelperErrorCode::ActionFailed, &note)
}

fn invalid_action_error(tool: &str, code: &str) -> HelperError {
    tracing::error!(
        tool,
        driver_error_code = code,
        "[computer] driver rejected the constructed action arguments"
    );
    HelperError::new(
        HelperErrorCode::ActionFailed,
        &format!(
            "The driver did not take the {tool} as iyw-claw sent it, so nothing was sent. \
 That is a fault in iyw-claw, not in the request — trying again will not help. Tell the user."
        ),
    )
}

fn unknown_action_error(tool: &str, result: &ToolCallResult) -> HelperError {
    let text: String = result.text().chars().take(300).collect();
    let note = if text.trim().is_empty() {
        format!("The {tool} did not happen.")
    } else {
        format!("The {tool} did not happen: {}", text.trim())
    };
    HelperError::new(HelperErrorCode::ActionFailed, &note)
}
