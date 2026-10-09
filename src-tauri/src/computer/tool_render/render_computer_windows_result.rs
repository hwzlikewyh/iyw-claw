pub fn render_computer_windows_result(outcome: &Value) -> Value {
    let windows = outcome.get("windows").and_then(Value::as_array);
    let screen = outcome
        .get("screen")
        .filter(|s| s.is_object())
        .map(|screen| {
            let s = |k: &str| screen.get(k).and_then(Value::as_str).unwrap_or("");
            format!(
                "The entire screen:\n  {}  [shared: {}]  — computer_screenshot shows all of it, \
                 what is never shared painted over; computer_click, computer_drag and \
                 computer_scroll take points from that picture, at the front as the user's own \
                 pointer. Keys and typing go to a window.\n\n",
                s("targetId"),
                s("level"),
            )
        })
        .unwrap_or_default();
    let text = match windows {
        Some(windows) if !windows.is_empty() => {
            let mut out = format!("{screen}Windows ({}):\n", windows.len());
            let mut any_unshared = false;
            for w in windows {
                let s = |k: &str| w.get(k).and_then(Value::as_str).unwrap_or("");
                let app = w.get("app");
                let app_s = |k: &str| app.and_then(|a| a.get(k)).and_then(Value::as_str).unwrap_or("");
                let b = |k: &str| {
                    w.get("bounds")
                        .and_then(|b| b.get(k))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                };
                let level = s("level");
                let readable = level == "read" || level == "control";
                let blocked = w.get("note").is_some();
                if !readable && !blocked {
                    any_unshared = true;
                }
                out.push_str(&format!(
                    "  {}  {} (pid {})  {:.0}×{:.0} at ({:.0}, {:.0})",
                    s("targetId"),
                    app_s("name"),
                    app.and_then(|a| a.get("pid")).and_then(Value::as_u64).unwrap_or(0),
                    b("width"),
                    b("height"),
                    b("x"),
                    b("y"),
                ));
                if w.get("minimized").and_then(Value::as_bool) == Some(true) {
                    out.push_str("  [minimized]");
                } else if w.get("hidden").and_then(Value::as_bool) == Some(true) {
                    out.push_str("  [hidden]");
                } else if w.get("onScreen").and_then(Value::as_bool) == Some(false) {
                    out.push_str("  [off screen]");
                }
                if blocked {
                    out.push_str(&format!("  [never shareable: {}]", s("note")));
                } else if readable && w.get("wholeScreen").and_then(Value::as_bool) == Some(true) {
                    out.push_str(&format!("  [shared: {level}, with the entire screen]"));
                } else if readable && w.get("wholeApp").and_then(Value::as_bool) == Some(true) {
                    out.push_str(&format!("  [shared: {level}, with its whole application]"));
                } else if readable {
                    out.push_str(&format!("  [shared: {level}]"));
                } else {
                    out.push_str("  [not shared]");
                }
                if let Some(title) = w.get("title").and_then(Value::as_str) {
                    out.push_str(&format!("  {title}"));
                }
                out.push('\n');
            }
            if any_unshared {
                out.push_str(
                    "\nA window marked \"not shared\" cannot be read. Ask the user to share it: in \
                     iyw-claw's status bar they open Computer use and press \"Share a window…\" — it \
                     is theirs to give.",
                );
            }
            if let Some(input) = outcome.get("input") {
                out.push('\n');
                out.push_str(computer_input_policy_line(input));
            }
            out
        }
        _ => {
            let none = outcome
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("No windows are open.");
            format!("{screen}{none}")
        }
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
