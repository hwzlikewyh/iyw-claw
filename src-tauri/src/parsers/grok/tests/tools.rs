use super::*;
use crate::parsers::grok::tools::{grok_mcp_input_preview, GROK_TOOL_INPUT_CAP};

const DELEGATE_UPDATES: &str = concat!(
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"委派构建"}}},"timestamp":1783584019}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"call-d","title":"use_tool","rawInput":{"tool_name":"codeg-mcp__delegate_to_agent","tool_input":{"agent_type":"codex","working_dir":"/w","task":"run build"}},"_meta":{"x.ai/tool":{"name":"use_tool"}}}},"timestamp":1783584029}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"call-d","status":"completed","rawOutput":{"type":"MCP","tool_name":"delegate_to_agent","server_name":"codeg-mcp","output":{"OkayOutput":"Delegation successful. task_id=2dc85849-5426-44f7."}}}},"timestamp":1783584122}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn"}},"timestamp":1783584129}"#,
    "\n",
);

#[test]
fn parses_turns_blocks_and_tool_result() {
    let detail = detail(UPDATES);
    assert_eq!(detail.turns.len(), 4);
    assert!(matches!(detail.turns[0].role, TurnRole::User));
    assert!(matches!(
        &detail.turns[1].blocks[0],
        ContentBlock::Thinking { text } if text == "Thinking about it"
    ));
    let last = &detail.turns[3];
    assert!(last.blocks.iter().any(|block| matches!(
        block,
        ContentBlock::ToolUse { tool_name, .. } if tool_name == "run_terminal_command"
    )));
    assert!(last.blocks.iter().any(|block| matches!(
        block,
        ContentBlock::ToolResult { output_preview, is_error, .. }
            if output_preview.as_deref() == Some("build ok") && !*is_error
    )));
}

#[test]
fn unwraps_use_tool_mcp_delegate_envelope() {
    let detail = detail(DELEGATE_UPDATES);
    let assistant = detail.turns.last().expect("assistant turn");
    let (name, input) = assistant
        .blocks
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolUse {
                tool_name,
                input_preview,
                ..
            } => Some((tool_name, input_preview.as_deref().unwrap_or_default())),
            _ => None,
        })
        .expect("tool use");
    assert_eq!(name, "codeg-mcp__delegate_to_agent");
    assert!(input.contains("\"task\":\"run build\""));
    assert!(!input.contains("tool_input"));
    assert!(assistant.blocks.iter().any(|block| matches!(
        block,
        ContentBlock::ToolResult { output_preview: Some(output), .. }
            if output.contains("task_id=2dc85849")
    )));
}

#[test]
fn use_tool_long_task_input_preview_stays_valid_json() {
    let updates = long_task_updates(&"x".repeat(GROK_TOOL_INPUT_CAP + 5_000));
    let detail = detail(&updates);
    let input = detail
        .turns
        .last()
        .unwrap()
        .blocks
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolUse { input_preview, .. } => input_preview.clone(),
            _ => None,
        })
        .expect("tool input");
    let parsed: Value = serde_json::from_str(&input).expect("valid JSON");
    assert_eq!(
        parsed.get("agent_type").and_then(Value::as_str),
        Some("codex")
    );
    assert!(parsed
        .get("task")
        .and_then(Value::as_str)
        .is_some_and(|task| !task.is_empty()));
    assert!(input.len() <= GROK_TOOL_INPUT_CAP);
}

#[test]
fn grok_mcp_input_preview_is_valid_and_bounded_for_compound_input() {
    let big = "x".repeat(GROK_TOOL_INPUT_CAP * 3);
    let input = serde_json::json!({
        "agent_type": "codex",
        "task": big,
        "working_dir": big,
        "notes": "行".repeat(GROK_TOOL_INPUT_CAP),
        "escaped": "\n".repeat(GROK_TOOL_INPUT_CAP),
        "list": [big, big, big],
    });
    let preview = grok_mcp_input_preview(&input).expect("preview");
    let parsed: Value = serde_json::from_str(&preview).expect("valid JSON");
    assert_eq!(
        parsed.get("agent_type").and_then(Value::as_str),
        Some("codex")
    );
    assert!(parsed
        .get("task")
        .and_then(Value::as_str)
        .is_some_and(|task| !task.is_empty()));
    assert!(preview.len() <= GROK_TOOL_INPUT_CAP);
}

fn long_task_updates(task: &str) -> String {
    format!(
        concat!(
            r#"{{"method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"user_message_chunk","content":{{"type":"text","text":"go"}}}}}},"timestamp":1783584019}}"#,
            "\n",
            r#"{{"method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"tool_call","toolCallId":"call-d","title":"use_tool","rawInput":{{"tool_name":"codeg-mcp__delegate_to_agent","tool_input":{{"agent_type":"codex","task":"{}"}}}}}}}},"timestamp":1783584029}}"#,
            "\n",
            r#"{{"method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"turn_completed","stop_reason":"end_turn"}}}},"timestamp":1783584129}}"#,
            "\n",
        ),
        task
    )
}
