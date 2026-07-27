pub(crate) fn unwrap_use_tool(
    raw_input: Option<&serde_json::Value>,
) -> Option<(String, serde_json::Value)> {
    let object = raw_input?.as_object()?;
    let tool_name = object
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.is_empty())?;
    let tool_input = object.get("tool_input")?;
    Some((tool_name.to_string(), tool_input.clone()))
}

fn mcp_output_text(raw_output: &serde_json::Value) -> Option<String> {
    if raw_output.get("type").and_then(serde_json::Value::as_str) != Some("MCP") {
        return None;
    }
    let output = raw_output.get("output")?;
    if let Some(text) = output.as_str().filter(|text| !text.is_empty()) {
        return Some(text.to_string());
    }
    output
        .as_object()?
        .values()
        .find_map(|value| value.as_str().filter(|text| !text.is_empty()))
        .map(str::to_string)
}

pub(crate) fn live_tool_output(
    content: &Option<String>,
    raw_output: &Option<serde_json::Value>,
) -> Option<String> {
    if content
        .as_deref()
        .is_some_and(|content| !content.trim().is_empty())
    {
        return None;
    }
    let raw_output = raw_output.as_ref()?;
    raw_output
        .get("output_for_prompt")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .or_else(|| mcp_output_text(raw_output))
}

