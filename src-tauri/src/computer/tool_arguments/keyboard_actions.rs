fn type_request(arguments: &Value, tool: &str) -> Result<ComputerActRequest, String> {
    let target = computer_element(arguments, tool)?;
    let text = computer_text(arguments, tool, "text")?;
    if text.is_empty() {
        return Err(format!("{tool}: `text` is empty"));
    }
    Ok(ComputerActRequest::Type {
        target,
        text,
        submit: computer_bool(arguments, tool, "submit")?,
    })
}

fn key_target(
    arguments: &Value,
    tool: &str,
) -> Result<Option<crate::computer::types::ElementTarget>, String> {
    match computer_target(arguments, tool)? {
        Some(crate::computer::types::AgentTarget::Element(e)) => Ok(Some(e)),
        Some(crate::computer::types::AgentTarget::Point(_)) => Err(format!(
            "{tool} presses a key on an element (`ref`) or on whatever has focus — not at a coordinate"
        )),
        None => Ok(None),
    }
}

fn key_request(arguments: &Value, tool: &str) -> Result<ComputerActRequest, String> {
    use crate::computer::keys::{Chord, Key};
    use crate::computer::types::{MAX_HOLD_MS, MAX_KEY_REPEAT};
    let key = arguments
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{tool} requires `key`, a string"))?;
    let chord = Chord {
        key: Key::parse(key).map_err(|e| format!("{tool}: {e}"))?,
        modifiers: computer_modifiers(arguments, tool)?,
    };
    let target = key_target(arguments, tool)?;
    if chord.types_text() && target.is_none() {
        return Err(format!(
            "{tool}: a key that types a character goes only into an element you name — \
                           pass its `ref` and `generation`, or type the text with computer_type"
        ));
    }
    if tool == "computer_hold_key" {
        if arguments.get("durationMs").is_none_or(Value::is_null) {
            return Err(format!(
                "{tool} requires `durationMs`, how long to hold the key (at most {MAX_HOLD_MS})"
            ));
        }
        Ok(ComputerActRequest::HoldKey {
            target,
            chord,
            duration_ms: computer_count(arguments, tool, "durationMs", 1..=MAX_HOLD_MS, 1)?,
        })
    } else {
        Ok(ComputerActRequest::Key {
            target,
            chord,
            repeat: computer_count(arguments, tool, "repeat", 1..=MAX_KEY_REPEAT, 1)?,
        })
    }
}
