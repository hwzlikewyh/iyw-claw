pub(crate) fn computer_text(arguments: &Value, tool: &str, key: &str) -> Result<String, String> {
    use crate::computer::types::MAX_ACTION_TEXT_CHARS;
    let text = arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{tool} requires `{key}`, a string"))?;
    if text.chars().count() > MAX_ACTION_TEXT_CHARS {
        return Err(format!(
            "{tool}: `{key}` may be at most {MAX_ACTION_TEXT_CHARS} characters; send it in pieces"
        ));
    }
    Ok(text.to_string())
}
