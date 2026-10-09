pub fn render_computer_launch_result(outcome: &Value) -> Value {
    let Some(app) = outcome.get("app").filter(|a| a.is_object()) else {
        return computer_refusal(outcome, "Nothing was started.");
    };
    let s = |k: &str| app.get(k).and_then(Value::as_str).unwrap_or("");
    let pid = app.get("pid").and_then(Value::as_u64).unwrap_or(0);
    let mut text = if pid > 0 {
        format!("Started {} (pid {pid}, {}).", s("name"), s("key"))
    } else {
        format!("Started {} ({}).", s("name"), s("key"))
    };
    if let Some(note) = outcome.get("note").and_then(Value::as_str) {
        text.push(' ');
        text.push_str(note);
    }
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
