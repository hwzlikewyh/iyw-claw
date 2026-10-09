use super::*;

const FIXED_ACTION_ERRORS: &[(&[&str], HelperErrorCode, &str)] = &[
    (
        &[
            "stale_element_token",
            "invalid_element_token",
            "conflicting_element_target",
        ],
        HelperErrorCode::StaleRef,
        "That ref is from a snapshot this window has moved past. Take a new \
             computer_snapshot and use a ref from it.",
    ),
    (
        &["screenshot_context_missing"],
        HelperErrorCode::StaleRef,
        NO_CAPTURE,
    ),
    (
        &["px_frame_mismatch"],
        HelperErrorCode::StaleRef,
        "The window changed while the point was being placed. Take a new \
             computer_screenshot and use a point from it.",
    ),
    (
        &[
            "window_not_found",
            "window_target_not_found",
            "window_id_not_found",
            "owner_pid_mismatch",
            "window_owner_pid_mismatch",
            "window_target_mismatch",
            "px_window_not_found",
        ],
        HelperErrorCode::NoSuchWindow,
        "the window is gone",
    ),
    (
        &["element_outside_target_window"],
        HelperErrorCode::OutOfTarget,
        "That element is not part of this window — a menu or panel of the application's \
             own, perhaps. Only what is inside the shared window can be acted on.",
    ),
    (
        &["point_outside_window"],
        HelperErrorCode::OutOfTarget,
        "That point is outside the window as it is now, so nothing was sent. Take a new \
             computer_screenshot and use a point from it.",
    ),
    (
        &["target_occluded"],
        HelperErrorCode::Occluded,
        "Another application's window is over that point, and the click would land on it \
             instead, so nothing was sent. Click an element by ref, or ask the user to move the \
             window that is in the way.",
    ),
    (
        &["off_space_or_ax_unresolved"],
        HelperErrorCode::Occluded,
        "The window is on another desktop (Space), or its contents cannot be reached right \
             now. Ask the user to bring it onto the current desktop.",
    ),
    (
        &[
            "minimized_or_hidden_window",
            "window_minimized",
            "element_not_visible",
        ],
        HelperErrorCode::Occluded,
        "The window is minimized or its application is hidden, so pointer and key input \
             cannot reach it. Such a window (computer_list_windows marks it minimized or hidden) \
             comes back with computer_restore — the user will see it — and then this can be \
             tried again. A click on an element by ref, or computer_set_value, may work as it \
             is.",
    ),
    (
        &["same_pid_keyboard_ambiguity"],
        HelperErrorCode::Occluded,
        "Its application has other windows open that the keys could reach instead, so no \
             keys are sent in the background. Use computer_set_value on the field, or a click \
             on an element by ref — or ask the user to close the application's other windows.",
    ),
    (
        &["popup_keyboard_grab"],
        HelperErrorCode::ActionFailed,
        "A pop-up of the application — a menu or a list — holds the keyboard, so the keys \
             were not sent. Choose from it or close it first, then try again.",
    ),
    (
        &["wm_chord_unavailable"],
        HelperErrorCode::ActionFailed,
        "The desktop's window manager takes that key combination for itself, so it was not \
             sent to the window.",
    ),
    (
        &["background_uipi_blocked"],
        HelperErrorCode::ActionFailed,
        HIGHER_RIGHTS,
    ),
    (
        &["input_delivery_unavailable"],
        HelperErrorCode::ActionFailed,
        "This window takes no typing from iyw-claw: Windows can accept it without it ever \
             reaching the prompt, so it is not sent, in the background or at the front. Ask the \
             user to type it, or run the command another way.",
    ),
    (
        &["type_text_incomplete"],
        HelperErrorCode::ActionFailed,
        "Only part of the text was typed. Take a new computer_snapshot to see what \
             arrived before typing the rest.",
    ),
    (
        &["input_busy"],
        HelperErrorCode::ActionFailed,
        "The window is busy with other input. Try again in a moment.",
    ),
    (
        &["set_value_unavailable"],
        HelperErrorCode::ActionFailed,
        "That element's value cannot be set directly here; nothing was changed. Type into it \
             with computer_type, or choose with clicks.",
    ),
    (
        &["element_bounds_unavailable"],
        HelperErrorCode::ActionFailed,
        "Where that element is cannot be read, so it was not clicked. Click a point on it \
             from a computer_screenshot, or an element inside it.",
    ),
    (
        &["ax_timeout"],
        HelperErrorCode::ActionFailed,
        "The application did not finish the action in time, so whether it went through \
             cannot be told — it may have. Read the window (computer_snapshot or \
             computer_screenshot) before doing it again.",
    ),
    (
        &["modified_pointer_unavailable"],
        HelperErrorCode::ActionFailed,
        "Keys cannot be held down through that pointer action on this desktop, so nothing \
             was sent. Do it without modifiers.",
    ),
];

pub(super) fn fixed_action_error(code: &str) -> Option<HelperError> {
    FIXED_ACTION_ERRORS
        .iter()
        .find(|(codes, _, _)| codes.contains(&code))
        .map(|(_, kind, note)| HelperError::new(*kind, *note))
}
