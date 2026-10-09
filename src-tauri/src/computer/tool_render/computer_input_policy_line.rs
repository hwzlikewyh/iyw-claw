pub(crate) fn computer_input_policy_line(input: &Value) -> &'static str {
    let front_allowed = input.get("foregroundAllowed").and_then(Value::as_bool) == Some(true);
    let front_default = input.get("default").and_then(Value::as_str) == Some("foreground");
    match (front_allowed, front_default) {
        (true, true) => {
            "Input: the user has each action bring its window to the front, then switch back to \
             the window they were in (on Linux it stays in front) — they will see it, and a click \
             may move their pointer. Pass `delivery: \"background\"` to leave the window where it \
             is."
        }
        (true, false) => {
            "Input: actions go to a window in the background, leaving it where it is. Where an \
             application will not take one that way, pass `delivery: \"foreground\"`: the user \
             allows a window to be brought to the front for that one action."
        }
        (false, _) => {
            "Input: actions go to a window in the background, leaving it where it is; the user \
             has switched off bringing windows to the front."
        }
    }
}
