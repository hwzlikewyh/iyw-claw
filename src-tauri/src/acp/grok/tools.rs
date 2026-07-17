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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_use_tool_to_the_real_mcp_name_and_input() {
        let raw = serde_json::json!({
            "tool_name": "iyw-claw-mcp__delegate_to_agent",
            "tool_input": {"agent_type": "codex", "task": "build"}
        });
        let (name, input) = unwrap_use_tool(Some(&raw)).expect("MCP envelope");

        assert_eq!(name, "iyw-claw-mcp__delegate_to_agent");
        assert_eq!(input["task"], "build");
        assert!(unwrap_use_tool(Some(&serde_json::json!({"command": "pnpm test"}))).is_none());
    }

    #[test]
    fn live_output_prefers_content_and_extracts_readable_fallbacks() {
        let terminal = Some(serde_json::json!({
            "output": [10, 62, 32],
            "output_for_prompt": "exit: 0\n\nok",
            "command": "pnpm test"
        }));
        assert_eq!(live_tool_output(&Some("ok".to_string()), &terminal), None);
        assert_eq!(
            live_tool_output(&None, &terminal).as_deref(),
            Some("exit: 0\n\nok")
        );

        let mcp = Some(serde_json::json!({
            "type": "MCP",
            "output": {"Empty": "", "OkayOutput": "task_id=abc"}
        }));
        assert_eq!(
            live_tool_output(&None, &mcp).as_deref(),
            Some("task_id=abc")
        );
    }
}
