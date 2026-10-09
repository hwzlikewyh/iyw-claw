pub fn render_computer_verify_result(outcome: &Value) -> Value {
    let Some(verify) = outcome.get("verify").filter(|v| v.is_object()) else {
        return computer_refusal(outcome, "The window could not be checked.");
    };
    let status = verify.get("status").and_then(Value::as_str).unwrap_or("unknown");
    let mut out = format!(
        "Verify on window {}: {status}",
        outcome.get("targetId").and_then(Value::as_str).unwrap_or("?")
    );
    out.push_str(&format!(
        " ({} sample(s), {} ms{}).",
        verify.get("samples").and_then(Value::as_u64).unwrap_or(0),
        verify.get("elapsedMs").and_then(Value::as_u64).unwrap_or(0),
        if verify.get("stable").and_then(Value::as_bool) == Some(true) {
            ", stable"
        } else {
            ""
        }
    ));
    if status == "unknown" {
        out.push_str(" Unknown is not success.");
    }
    for p in verify
        .get("predicates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.push_str(&format!(
            "\n  #{} {}",
            p.get("index").and_then(Value::as_u64).unwrap_or(0),
            p.get("status").and_then(Value::as_str).unwrap_or("unknown")
        ));
        if let Some(reason) = p.get("unknownReason").and_then(Value::as_str) {
            out.push_str(&format!(" ({reason})"));
        }
    }
    json!({
        "content": [{ "type": "text", "text": out }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
