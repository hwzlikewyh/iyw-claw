pub(crate) fn computer_target_id(arguments: &Value, tool: &str) -> Result<String, String> {
    arguments
        .get("targetId")
        .or_else(|| arguments.get("target_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!("{tool} requires a non-empty `targetId` string (from computer_list_windows)")
        })
}
