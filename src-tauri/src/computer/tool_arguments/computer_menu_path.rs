pub(crate) fn computer_menu_path(arguments: &Value, tool: &str) -> Result<Vec<String>, String> {
    let invalid = || {
        format!(
            "{tool} requires `path`: the titles from the menu bar down to the command, e.g. \
             [\"File\", \"Export\", \"PDF…\"] — one to {MAX_MENU_PATH} of them"
        )
    };
    let items = arguments
        .get("path")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if items.is_empty() || items.len() > MAX_MENU_PATH {
        return Err(invalid());
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_string)
                .ok_or_else(invalid)
        })
        .collect()
}
