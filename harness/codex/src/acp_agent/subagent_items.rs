use serde_json::{json, Value};

use super::acp_mapping::Update;

pub(super) fn map(item: &Value, completed: bool) -> Option<Update> {
    let kind = item.get("type")?.as_str()?;
    let (title, input, meta) = match kind {
        "collabAgentToolCall" => (
            item.get("tool")?.as_str()?.to_string(),
            json!({
                "prompt": item.get("prompt"), "senderThreadId": item.get("senderThreadId"),
                "receiverThreadIds": item.get("receiverThreadIds"), "agentsStates": item.get("agentsStates"),
                "model": item.get("model"), "reasoningEffort": item.get("reasoningEffort"),
                "status": item.get("status"),
            }),
            json!({ "codex": { "collaboration": {
                "tool": item.get("tool"), "senderThreadId": item.get("senderThreadId"),
                "receiverThreadIds": item.get("receiverThreadIds"),
            } } }),
        ),
        "subAgentActivity" => (
            format!(
                "agent {}",
                item.get("agentPath").and_then(Value::as_str).unwrap_or("")
            ),
            json!({ "agentThreadId": item.get("agentThreadId"), "agentPath": item.get("agentPath"), "activityKind": item.get("kind") }),
            json!({ "codex": { "subagent": { "threadId": item.get("agentThreadId"), "path": item.get("agentPath") } } }),
        ),
        _ => return None,
    };
    let status = match item.get("status").and_then(Value::as_str) {
        Some("failed" | "declined" | "cancelled" | "canceled") => "failed",
        Some("completed" | "success") => "completed",
        _ if completed => "completed",
        _ => "in_progress",
    };
    Some(Update {
        method: if completed {
            "tool_call_update"
        } else {
            "tool_call"
        },
        params: json!({
            "toolCallId": item.get("id")?, "kind": "other", "title": title,
            "status": status, "rawInput": input, "rawOutput": item.get("agentsStates"), "_meta": meta,
        }),
    })
}
