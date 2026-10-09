pub fn render_computer_clipboard_result(outcome: &Value) -> Value {
    let text = if let Some(read) = outcome.get("text").and_then(Value::as_str) {
        read.to_string()
    } else if outcome.get("written").and_then(Value::as_bool) == Some(true) {
        outcome
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or("Written.")
            .to_string()
    } else {
        return computer_refusal(outcome, "Nothing was done.");
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
