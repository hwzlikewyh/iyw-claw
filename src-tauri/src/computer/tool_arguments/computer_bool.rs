pub(crate) fn computer_bool(arguments: &Value, tool: &str, key: &str) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("{tool}: `{key}` must be true or false, not {other}")),
    }
}
