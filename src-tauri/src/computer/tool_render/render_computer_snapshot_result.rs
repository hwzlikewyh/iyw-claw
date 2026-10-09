pub fn render_computer_snapshot_result(outcome: &Value) -> Value {
    let Some(snapshot) = outcome.get("snapshot").filter(|s| s.is_object()) else {
        return computer_refusal(outcome, "The window could not be read.");
    };
    let s = |k: &str| snapshot.get(k).and_then(Value::as_str).unwrap_or("");
    let mut out = format!("Window {}", s("targetId"));
    if let Some(title) = snapshot.get("title").and_then(Value::as_str) {
        out.push_str(&format!(" — {title}"));
    }
    out.push_str(&format!(
        "\nGeneration {} · {} elements\n",
        s("generation"),
        snapshot.get("elementCount").and_then(Value::as_u64).unwrap_or(0)
    ));
    if let Some(degraded) = snapshot.get("degraded").and_then(Value::as_str) {
        out.push_str(&format!("The tree is incomplete: {degraded}\n"));
    }
    if snapshot.get("truncated").and_then(Value::as_bool) == Some(true) {
        out.push_str(
            "The tree below stops early — pass a larger `maxChars` (or 0 for all of it), or a \
             `query` to keep only the lines you need.\n",
        );
    }
    out.push('\n');
    out.push_str(s("tree"));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(COMPUTER_DATA_NOT_INSTRUCTIONS);
    json!({
        "content": [{ "type": "text", "text": out }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
