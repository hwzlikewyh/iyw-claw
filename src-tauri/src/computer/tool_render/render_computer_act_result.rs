pub fn render_computer_act_result(outcome: &Value) -> Value {
    let Some(action) = outcome.get("action").filter(|a| a.is_object()) else {
        return computer_refusal(outcome, "Nothing was done.");
    };
    let s = |k: &str| action.get(k).and_then(Value::as_str).unwrap_or("");
    let effect = match s("effect") {
        "confirmed" => "done, and confirmed by reading the window back",
        "partial" => "done only in part",
        "suspected_noop" => {
            "delivered, but it appears to have changed nothing — look before trying again"
        }
        _ => {
            "delivered; whether it took effect could not be read back — check with \
             computer_verify or a new computer_snapshot"
        }
    };
    let mut out = format!(
        "Window {}: {effect}.",
        outcome.get("targetId").and_then(Value::as_str).unwrap_or("?")
    );
    let route = match s("route") {
        "accessibility" => Some("through the accessibility interface"),
        "synthetic_events" => Some("as synthesized input events"),
        "global_input" | "trusted_input" => Some("as input events"),
        "dom" => Some("through the page"),
        _ => None,
    };
    match (s("delivery") == "foreground", route) {
        (true, Some(route)) => out.push_str(&format!(
            " Delivered with the window brought to the front for it, {route}."
        )),
        (true, None) => out.push_str(" The window was brought to the front for it."),
        (false, Some(route)) => out.push_str(&format!(" Delivered in the background, {route}.")),
        (false, None) => {}
    }
    if let Some(n) = action.get("presses").and_then(Value::as_u64) {
        out.push_str(&format!(" The key was pressed {n} times."));
    }
    match action.get("submitted").and_then(Value::as_bool) {
        Some(true) => out.push_str(" Return was pressed after the text."),
        Some(false) => match action.get("submitNote").and_then(Value::as_str) {
            Some(why) => out.push_str(&format!(
                " Return could not be pressed after the text: {why} Press it with \
                 computer_press_key once that allows."
            )),
            None => out.push_str(
                " Return could not be pressed after the text; press it with computer_press_key.",
            ),
        },
        None => {}
    }
    out.push_str(" Take a new computer_snapshot or computer_screenshot to see the result.");
    json!({
        "content": [{ "type": "text", "text": out }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
