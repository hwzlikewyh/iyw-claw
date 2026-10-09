pub(crate) fn computer_count(
    arguments: &Value,
    tool: &str,
    key: &str,
    range: std::ops::RangeInclusive<u32>,
    default: u32,
) -> Result<u32, String> {
    match computer_optional_u32(arguments, tool, key)? {
        None => Ok(default),
        Some(n) if range.contains(&n) => Ok(n),
        Some(n) => Err(format!(
            "{tool}: `{key}` must be from {} to {}, not {n}",
            range.start(),
            range.end()
        )),
    }
}
