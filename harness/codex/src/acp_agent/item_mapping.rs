use std::collections::HashMap;

use serde_json::{json, Value};

use super::acp_mapping::Update;

const MAX_TOOL_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_TOOL_OUTPUTS: usize = 256;
const TRUNCATED_OUTPUT_PREFIX: &str = "[earlier output omitted]\n";

#[derive(Default)]
pub(super) struct ItemProjection {
    outputs: HashMap<String, String>,
}

impl ItemProjection {
    pub(super) fn map(&mut self, method: &str, params: &Value) -> Option<Update> {
        match method {
            "item/started" => item_update(params, false),
            "item/completed" => {
                let update = item_update(params, true);
                if let Some(id) = item_id(params) {
                    self.outputs.remove(id);
                }
                update
            }
            "item/commandExecution/outputDelta" | "item/fileChange/outputDelta" => {
                self.output_delta(params)
            }
            "item/mcpToolCall/progress" => progress_update(params),
            _ => None,
        }
    }

    fn output_delta(&mut self, params: &Value) -> Option<Update> {
        let id = required_str(params, "itemId", "item_id")?;
        let delta = params.get("delta")?.as_str()?;
        let delta = bounded_delta(delta);
        if !self.outputs.contains_key(id) && self.outputs.len() >= MAX_TOOL_OUTPUTS {
            if let Some(evicted) = self.outputs.keys().next().cloned() {
                self.outputs.remove(&evicted);
            }
        }
        let output = self.outputs.entry(id.to_string()).or_default();
        output.push_str(&delta);
        truncate_output(output);
        Some(tool_delta(id, &delta))
    }
}

fn item_update(params: &Value, completed: bool) -> Option<Update> {
    let item = params.get("item")?;
    let kind = item.get("type")?.as_str()?;
    if matches!(kind, "userMessage" | "agentMessage" | "reasoning" | "plan") {
        return None;
    }
    let id = item.get("id")?.as_str()?;
    if completed {
        let status = completed_status(item);
        return Some(tool_update(id, Some(status), completed_output(item)));
    }
    let (title, tool_kind) = tool_identity(kind, item)?;
    Some(Update {
        method: "tool_call",
        params: json!({
            "toolCallId": id,
            "title": title,
            "kind": tool_kind,
            "status": started_status(item),
            "rawInput": item,
        }),
    })
}

fn tool_identity(kind: &str, item: &Value) -> Option<(String, &'static str)> {
    let identity = match kind {
        "commandExecution" => (item.get("command")?.as_str()?.to_string(), "execute"),
        "fileChange" => ("File changes".to_string(), "edit"),
        "mcpToolCall" => (
            format!(
                "{}: {}",
                item.get("server")?.as_str()?,
                item.get("tool")?.as_str()?
            ),
            "fetch",
        ),
        "dynamicToolCall" => (item.get("tool")?.as_str()?.to_string(), "other"),
        "collabAgentToolCall" | "subAgentActivity" => ("agent".to_string(), "other"),
        "webSearch" => ("Web search".to_string(), "search"),
        "imageView" => ("View image".to_string(), "read"),
        "imageGeneration" => ("Image generation".to_string(), "other"),
        "functionCallOutput" => ("Tool output".to_string(), "other"),
        "sleep" => ("Wait".to_string(), "other"),
        "enteredReviewMode" | "exitedReviewMode" => ("Review".to_string(), "think"),
        "contextCompaction" => ("Compact context".to_string(), "think"),
        _ => return None,
    };
    Some(identity)
}

fn progress_update(params: &Value) -> Option<Update> {
    let id = required_str(params, "itemId", "item_id")?;
    let message = params.get("message")?.as_str()?;
    Some(tool_update(
        id,
        None,
        Some(Value::String(message.to_string())),
    ))
}

fn tool_update(id: &str, status: Option<&str>, raw_output: Option<Value>) -> Update {
    let mut params = json!({ "toolCallId": id });
    let object = params.as_object_mut().expect("tool update is an object");
    if let Some(status) = status {
        object.insert("status".into(), Value::String(status.to_string()));
    }
    if let Some(raw_output) = raw_output {
        object.insert("rawOutput".into(), raw_output);
    }
    Update {
        method: "tool_call_update",
        params,
    }
}

fn tool_delta(id: &str, delta: &str) -> Update {
    Update {
        method: "tool_call_update",
        params: json!({
            "toolCallId": id,
            "rawOutput": delta,
            "rawOutputAppend": true,
        }),
    }
}

fn started_status(item: &Value) -> &'static str {
    item.get("status")
        .and_then(Value::as_str)
        .map(map_status)
        .unwrap_or("in_progress")
}

fn completed_status(item: &Value) -> &'static str {
    match item.get("status").and_then(Value::as_str) {
        Some("failed" | "declined" | "cancelled" | "canceled") => "failed",
        _ => "completed",
    }
}

fn map_status(status: &str) -> &'static str {
    match status {
        "pending" => "pending",
        "inProgress" | "in_progress" => "in_progress",
        "completed" | "success" => "completed",
        "failed" | "declined" | "cancelled" | "canceled" => "failed",
        _ => "in_progress",
    }
}

fn completed_output(item: &Value) -> Option<Value> {
    for field in [
        "aggregatedOutput",
        "result",
        "error",
        "contentItems",
        "savedPath",
        "agentsStates",
        "output",
    ] {
        if let Some(value) = item.get(field).filter(|value| !value.is_null()) {
            return Some(value.clone());
        }
    }
    None
}

fn item_id(params: &Value) -> Option<&str> {
    params.pointer("/item/id")?.as_str()
}

fn required_str<'a>(params: &'a Value, camel: &str, snake: &str) -> Option<&'a str> {
    params
        .get(camel)
        .or_else(|| params.get(snake))
        .and_then(Value::as_str)
}

fn truncate_output(output: &mut String) {
    if output.len() <= MAX_TOOL_OUTPUT_BYTES {
        return;
    }
    let keep = MAX_TOOL_OUTPUT_BYTES.saturating_sub(TRUNCATED_OUTPUT_PREFIX.len());
    let mut start = output.len().saturating_sub(keep);
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }
    let tail = output[start..].to_string();
    output.clear();
    output.push_str(TRUNCATED_OUTPUT_PREFIX);
    output.push_str(&tail);
}

fn bounded_delta(delta: &str) -> String {
    if delta.len() <= MAX_TOOL_OUTPUT_BYTES {
        return delta.to_string();
    }
    let keep = MAX_TOOL_OUTPUT_BYTES.saturating_sub(TRUNCATED_OUTPUT_PREFIX.len());
    let mut start = delta.len().saturating_sub(keep);
    while start < delta.len() && !delta.is_char_boundary(start) {
        start += 1;
    }
    let tail = &delta[start..];
    let mut bounded = String::with_capacity(TRUNCATED_OUTPUT_PREFIX.len() + tail.len());
    bounded.push_str(TRUNCATED_OUTPUT_PREFIX);
    bounded.push_str(tail);
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_command_lifecycle_and_cumulative_output() {
        let mut projection = ItemProjection::default();
        let started = projection
            .map(
                "item/started",
                &json!({"item": {"type": "commandExecution", "id": "cmd", "command": "cargo test", "status": "inProgress"}}),
            )
            .expect("command starts");
        assert_eq!(started.method, "tool_call");
        assert_eq!(started.params["kind"], "execute");
        let first = projection
            .map(
                "item/commandExecution/outputDelta",
                &json!({"itemId": "cmd", "delta": "one"}),
            )
            .expect("first output");
        let second = projection
            .map(
                "item/commandExecution/outputDelta",
                &json!({"itemId": "cmd", "delta": "two"}),
            )
            .expect("second output");
        assert_eq!(first.params["rawOutput"], "one");
        assert_eq!(first.params["rawOutputAppend"], true);
        assert_eq!(second.params["rawOutput"], "two");
        assert_eq!(second.params["rawOutputAppend"], true);
    }

    #[test]
    fn maps_file_completion_without_duplicate_agent_text() {
        let mut projection = ItemProjection::default();
        let completed = projection
            .map(
                "item/completed",
                &json!({"item": {"type": "fileChange", "id": "edit", "status": "completed", "changes": []}}),
            )
            .expect("file completion maps");
        assert_eq!(completed.params["status"], "completed");
        assert!(projection
            .map(
                "item/started",
                &json!({"item": {"type": "agentMessage", "id": "msg", "text": "hello"}}),
            )
            .is_none());
    }

    #[test]
    fn bounds_one_oversized_output_delta() {
        let mut projection = ItemProjection::default();
        let update = projection
            .map(
                "item/commandExecution/outputDelta",
                &json!({"itemId": "cmd", "delta": "x".repeat(MAX_TOOL_OUTPUT_BYTES + 10)}),
            )
            .expect("oversized output maps");
        assert!(update.params["rawOutput"].as_str().unwrap().len() <= MAX_TOOL_OUTPUT_BYTES);
        assert_eq!(update.params["rawOutputAppend"], true);
    }
}
