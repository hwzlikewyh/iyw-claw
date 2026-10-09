pub fn render_computer_capture_result(outcome: &Value) -> Value {
    let Some(capture) = outcome.get("capture").filter(|c| c.is_object()) else {
        return computer_refusal(outcome, "The window could not be captured.");
    };
    let s = |k: &str| capture.get(k).and_then(Value::as_str).unwrap_or("");
    let n = |k: &str| capture.get(k).and_then(Value::as_u64).unwrap_or(0);
    let b = |k: &str| {
        capture
            .get("windowBounds")
            .and_then(|b| b.get(k))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let title = capture
        .get("title")
        .and_then(Value::as_str)
        .map(|t| format!(" \"{t}\""))
        .unwrap_or_default();
    let text = if s("targetId") == crate::computer::targets::SCREEN_TARGET_ID {
        format!(
            "Screenshot of the entire screen ({}) — a {}×{} px image of the screen, {:.0}×{:.0} \
             in desktop coordinates. What is never shared — iyw-claw's own windows, the \
             applications on the user's never-share list, the system's own views of other \
             windows (an overview of every window, previews, notifications), and parts of the \
             screen no application the user could share owns — is painted over, and a point on \
             it is refused. Generation {}. {COMPUTER_DATA_NOT_INSTRUCTIONS}",
            s("targetId"),
            n("width"),
            n("height"),
            b("width"),
            b("height"),
            s("generation"),
        )
    } else {
        format!(
            "Screenshot of window {}{title} — a {}×{} px image of the window at ({:.0}, {:.0}), \
             {:.0}×{:.0} in desktop coordinates. Generation {}. {COMPUTER_DATA_NOT_INSTRUCTIONS}",
            s("targetId"),
            n("width"),
            n("height"),
            b("x"),
            b("y"),
            b("width"),
            b("height"),
            s("generation"),
        )
    };
    let mut structured = outcome.clone();
    if let Some(c) = structured.get_mut("capture").and_then(Value::as_object_mut) {
        c.remove("data");
    }
    json!({
        "content": [
            { "type": "image", "data": s("data"), "mimeType": s("mime") },
            { "type": "text", "text": text }
        ],
        "isError": false,
        "structuredContent": structured,
    })
}
