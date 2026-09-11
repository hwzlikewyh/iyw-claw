use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::models::{ContentBlock, MessageRole, UnifiedMessage};

#[derive(Default)]
pub(super) struct DescribedCommands {
    direct: HashSet<String>,
    started: HashSet<String>,
    completed: HashSet<String>,
}

impl DescribedCommands {
    pub(super) fn normalize(&mut self, record: &mut Value) {
        if record["type"] == "response_item" && record["payload"]["type"] == "function_call" {
            if let Some(id) = record["payload"]["call_id"].as_str() {
                self.direct.insert(id.to_string());
            }
            return;
        }
        let Some((item, completed, id, description)) = described_event(record) else {
            return;
        };
        // 直接调用已有 function_call/result；仅补齐 Code Mode 内部命令的历史。
        if self.direct.contains(id) {
            return;
        }
        if completed && !self.completed.insert(id.to_string()) {
            return;
        }
        let first = self.started.insert(id.to_string());
        let mut payload = if first {
            json!({
                "type": "function_call", "name": "exec_command", "call_id": id,
                "arguments": { "command": item["command"], "description": description },
            })
        } else if completed {
            command_result(item, id)
        } else {
            return;
        };
        if first && completed {
            payload["_iyw_command_result"] = command_result(item, id);
        }
        record["type"] = json!("response_item");
        record["payload"] = payload;
    }
}

fn described_event(record: &Value) -> Option<(&Value, bool, &str, &str)> {
    let (item, completed) = command_event(record)?;
    let id = item.get("id").or_else(|| item.get("call_id"))?.as_str()?;
    let description = item["description"]
        .as_str()
        .filter(|text| !text.trim().is_empty())?;
    Some((item, completed, id, description))
}

fn command_event(record: &Value) -> Option<(&Value, bool)> {
    if record["type"] != "event_msg" {
        return None;
    }
    let payload = &record["payload"];
    match payload["type"].as_str()? {
        "item_started" | "item_completed" if payload["item"]["type"] == "CommandExecution" => {
            Some((&payload["item"], payload["type"] == "item_completed"))
        }
        "exec_command_begin" => Some((payload, false)),
        "exec_command_end" => Some((payload, true)),
        _ => None,
    }
}

fn command_result(item: &Value, id: &str) -> Value {
    let status = item["status"].as_str().unwrap_or_default();
    let is_error = matches!(status, "failed" | "declined")
        || item["exit_code"].as_i64().is_some_and(|code| code != 0);
    json!({
        "type": "function_call_output", "call_id": id,
        "output": item["aggregated_output"].as_str().or_else(|| item["formatted_output"].as_str()),
        "is_error": is_error,
    })
}

pub(super) fn command_input_preview(arguments: Value) -> Option<String> {
    if arguments["description"]
        .as_str()
        .is_some_and(|text| !text.trim().is_empty())
    {
        return serde_json::to_string(&arguments).ok();
    }
    arguments["cmd"].as_str().map(str::to_string)
}

pub(super) fn append_completed_command(
    payload: &Value,
    messages: &mut Vec<UnifiedMessage>,
    timestamp: DateTime<Utc>,
) {
    let Some(result) = payload.get("_iyw_command_result") else {
        return;
    };
    messages.push(UnifiedMessage {
        id: format!("tool-result-{}", messages.len()),
        role: MessageRole::Tool,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: result["call_id"].as_str().map(str::to_string),
            output_preview: result["output"].as_str().map(str::to_string),
            is_error: result["is_error"].as_bool().unwrap_or(false),
            agent_stats: None,
            images: Vec::new(),
        }],
        timestamp,
        usage: None,
        duration_ms: None,
        model: None,
        completed_at: Some(timestamp),
    });
}
