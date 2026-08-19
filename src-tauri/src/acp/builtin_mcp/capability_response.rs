use rmcp::model::{CallToolResult, RawContent};
use serde_json::Value;

use super::capability_registry::stable_capability_id;

const DELEGATION_FOLLOW_UP_TOOLS: [&str; 2] = ["get_delegation_status", "cancel_delegation"];

pub(super) fn rewrite_result(result: &mut CallToolResult) {
    for content in &mut result.content {
        if let RawContent::Text(text) = &mut content.raw {
            text.text = invocation_text(&text.text);
        }
    }
    if let Some(value) = result.structured_content.as_mut() {
        rewrite_value(value);
    }
}

pub(super) fn rewrite_error(message: String, data: Option<Value>) -> (String, Option<Value>) {
    let mut data = data;
    if let Some(value) = data.as_mut() {
        rewrite_value(value);
    }
    (invocation_text(&message), data)
}

fn rewrite_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = invocation_text(text),
        Value::Array(values) => values.iter_mut().for_each(rewrite_value),
        Value::Object(values) => values.values_mut().for_each(rewrite_value),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn invocation_text(text: &str) -> String {
    DELEGATION_FOLLOW_UP_TOOLS
        .iter()
        .filter_map(|tool_name| {
            stable_capability_id(tool_name).map(|capability_id| (*tool_name, capability_id))
        })
        .fold(text.to_string(), |value, (tool_name, capability_id)| {
            value.replace(
                tool_name,
                &format!("invoke_iyw_capability with capability_id {capability_id}"),
            )
        })
}
