pub(crate) fn computer_generation(arguments: &Value, tool: &str, what: &str) -> Result<String, String> {
    arguments
        .get("generation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "{tool}: a `{what}` needs the `generation` of the {} it came from",
                if what == "ref" {
                    "computer_snapshot"
                } else {
                    "computer_screenshot"
                }
            )
        })
}
