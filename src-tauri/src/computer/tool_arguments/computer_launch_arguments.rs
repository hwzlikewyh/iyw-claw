pub(crate) fn computer_launch_arguments(
    arguments: &Value,
) -> Result<(Option<String>, Option<String>), String> {
    const TOOL: &str = "computer_launch_app";
    if let Some(unknown) = arguments
        .as_object()
        .into_iter()
        .flat_map(|o| o.keys())
        .find(|k| !matches!(k.as_str(), "name" | "key"))
    {
        return Err(format!(
            "{TOOL} takes no argument `{unknown}`; it takes `name` or `key`"
        ));
    }
    let text = |key: &str| -> Result<Option<String>, String> {
        match arguments.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(v)) => {
                let v = v.trim();
                if v.is_empty() || v.chars().count() > MAX_LAUNCH_NAME {
                    Err(format!(
                        "{TOOL}: `{key}` must be a name of 1 to {MAX_LAUNCH_NAME} characters"
                    ))
                } else {
                    Ok(Some(v.to_string()))
                }
            }
            Some(_) => Err(format!("{TOOL}: `{key}` must be a string")),
        }
    };
    let (name, key) = (text("name")?, text("key")?);
    if name.is_none() && key.is_none() {
        return Err(format!(
            "{TOOL} requires `name` (as the system lists the application) or `key` (its bundle \
             identifier or path)"
        ));
    }
    Ok((name, key))
}
