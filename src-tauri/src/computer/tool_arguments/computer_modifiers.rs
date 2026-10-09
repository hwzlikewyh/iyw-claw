pub(crate) fn computer_modifiers(
    arguments: &Value,
    tool: &str,
) -> Result<crate::computer::keys::Modifiers, String> {
    use crate::computer::keys::Modifiers;
    match arguments.get("modifiers") {
        None | Some(Value::Null) => Ok(Modifiers::default()),
        Some(Value::Array(items)) => {
            let names = items
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{tool}: `modifiers` must be an array of names"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Modifiers::parse(&names).map_err(|e| format!("{tool}: {e}"))
        }
        Some(other) => Err(format!(
            "{tool}: `modifiers` must be an array of names, not {other}"
        )),
    }
}
