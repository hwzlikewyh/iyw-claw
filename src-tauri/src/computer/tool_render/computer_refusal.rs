pub(crate) fn computer_refusal(outcome: &Value, fallback: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": outcome.get("note").and_then(Value::as_str).unwrap_or(fallback),
        }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
