use serde_json::Value;

use crate::acp::session_state::{SessionState, ToolCallOutput, ToolCallState, ToolCallStatus};
use crate::acp::types::UserMessageBlock;

const EXPLICIT_CREATE_PHRASES: &[&str] = &[
    "创建定时任务", "新建定时任务", "建立定时任务", "设置定时任务",
    "安排定时任务", "添加定时任务", "写入定时任务", "直接创建任务",
    "create scheduled task", "schedule this task",
];
const RECURRING_TERMS: &[&str] = &["每天", "每日", "每周", "每月", "定时", "按排期"];
const AUTOMATION_TERMS: &[&str] = &["自动发布", "自动执行", "定时发布", "定时执行"];
const REQUEST_TERMS: &[&str] = &["帮我", "替我", "请", "直接", "按这个", "按照这个"];
const EXPLANATION_TERMS: &[&str] = &["怎么", "如何", "为什么", "是什么", "教程", "建议", "how to"];
const MAX_RESULT_DEPTH: u8 = 5;

pub(super) fn create_was_requested(state: &SessionState) -> bool {
    let text = state.pending_user_message.iter()
        .flat_map(|message| &message.blocks)
        .filter_map(|block| match block {
            UserMessageBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>().join(" ").to_lowercase();
    if text.is_empty() { return false; }
    if contains_any(&text, EXPLICIT_CREATE_PHRASES) { return true; }
    !contains_any(&text, EXPLANATION_TERMS)
        && contains_any(&text, RECURRING_TERMS)
        && contains_any(&text, AUTOMATION_TERMS)
        && contains_any(&text, REQUEST_TERMS)
}

pub(super) fn has_verified_create(state: &SessionState) -> bool {
    state.active_tool_calls.values().any(|call| {
        is_create_call(call)
            && call.status == ToolCallStatus::Completed
            && call.output.as_ref().is_some_and(output_has_receipt)
    })
}

fn contains_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}

fn is_create_call(call: &ToolCallState) -> bool {
    let label = call.label.to_lowercase().replace('-', "_").replace(' ', "_");
    if label.ends_with("create_scheduled_task") || call.label.contains("创建定时任务") {
        return true;
    }
    call.input.as_ref().is_some_and(|input| {
        let input = serde_json::to_string(input).unwrap_or_default();
        matches!(
            crate::acp::builtin_mcp::invoked_tool_name(&Some(input)),
            crate::acp::builtin_mcp::GatewayToolIdentity::Resolved(name)
                if name == "create_scheduled_task"
        )
    })
}

fn output_has_receipt(output: &ToolCallOutput) -> bool {
    match output {
        ToolCallOutput::Json { value } => contains_receipt(value, MAX_RESULT_DEPTH),
        ToolCallOutput::Text { content } => parse_result_text(content)
            .is_some_and(|value| contains_receipt(&value, MAX_RESULT_DEPTH)),
        ToolCallOutput::Error { .. } => false,
    }
}

fn parse_result_text(text: &str) -> Option<Value> {
    let body = text.rsplit_once("Output:\r\n")
        .or_else(|| text.rsplit_once("Output:\n"))
        .map(|(_, output)| output).unwrap_or(text).trim();
    serde_json::from_str(body).ok()
}

fn contains_receipt(value: &Value, depth: u8) -> bool {
    if depth == 0 || is_verified_receipt(value) { return is_verified_receipt(value); }
    if let Some(object) = value.as_object() {
        for key in ["structuredContent", "structured_content", "result", "output", "Ok", "ok"] {
            if object.get(key).is_some_and(|nested| contains_receipt(nested, depth - 1)) {
                return true;
            }
        }
        if let Some(items) = object.get("content").and_then(Value::as_array) {
            return items.iter().any(|item| {
                item.get("text").and_then(Value::as_str).and_then(parse_result_text)
                    .is_some_and(|nested| contains_receipt(&nested, depth - 1))
            });
        }
    }
    value.as_str().and_then(parse_result_text)
        .is_some_and(|nested| contains_receipt(&nested, depth - 1))
}

fn is_verified_receipt(value: &Value) -> bool {
    value.get("status").and_then(Value::as_str) == Some("verified")
        && value.get("verified").and_then(Value::as_bool) == Some(true)
        && value.get("task").and_then(|task| task.get("id"))
            .and_then(Value::as_i64).is_some_and(|id| id > 0)
}
