pub(crate) fn computer_choice<'a>(
    arguments: &Value,
    tool: &str,
    key: &str,
    allowed: &[&'a str],
) -> Result<Option<&'a str>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => allowed
            .iter()
            .find(|a| **a == s.as_str())
            .copied()
            .map(Some)
            .ok_or_else(|| format!("{tool}: `{key}` must be one of {}", allowed.join(", "))),
        Some(other) => Err(format!(
            "{tool}: `{key}` must be one of {}, not {other}",
            allowed.join(", ")
        )),
    }
}
