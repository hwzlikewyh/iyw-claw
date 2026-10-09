pub fn render_computer_apps_result(outcome: &Value) -> Value {
    let apps = outcome.get("apps").and_then(Value::as_array);
    let text = match apps {
        Some(apps) if !apps.is_empty() => {
            let mut out = format!("Running applications ({}):\n", apps.len());
            for app in apps {
                let s = |k: &str| app.get(k).and_then(Value::as_str).unwrap_or("");
                out.push_str(&format!(
                    "  {}  pid {}  {}",
                    s("name"),
                    app.get("pid").and_then(Value::as_u64).unwrap_or(0),
                    s("key"),
                ));
                if app.get("active").and_then(Value::as_bool) == Some(true) {
                    out.push_str("  [frontmost]");
                }
                if let Some(level) = app.get("level").and_then(Value::as_str) {
                    out.push_str(&format!("  [shared as a whole: {level}]"));
                }
                if let Some(note) = app.get("note").and_then(Value::as_str) {
                    out.push_str(&format!("  — {note}"));
                }
                out.push('\n');
            }
            out.push_str("\nUse computer_list_windows to see their windows.");
            out
        }
        _ => outcome
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or("No applications are running.")
            .to_string(),
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
