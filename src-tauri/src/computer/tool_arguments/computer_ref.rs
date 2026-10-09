pub(crate) fn computer_ref(value: &Value, tool: &str) -> Result<u32, String> {
    let parsed = match value {
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(s) => s
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<u32>()
            .ok(),
        _ => None,
    };
    parsed.ok_or_else(|| {
        format!(
            "{tool}: `ref` must be an element's number from computer_snapshot (the N in `[N]`), \
             not {value}"
        )
    })
}
