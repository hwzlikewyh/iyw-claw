use serde_json::Value;

use crate::parsers::truncate_str;

pub(super) const GROK_TOOL_OUTPUT_CAP: usize = 100_000;
pub(super) const GROK_TOOL_INPUT_CAP: usize = 8_000;

pub(super) fn update_text(update: &Value) -> String {
    update
        .pointer("/content/text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(super) fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(super) fn unwrap_use_tool(raw_input: Option<&Value>) -> Option<(&str, &Value)> {
    let object = raw_input?.as_object()?;
    let tool_name = object
        .get("tool_name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())?;
    Some((tool_name, object.get("tool_input")?))
}

fn grok_mcp_output_text(raw_output: &Value) -> Option<String> {
    if raw_output.get("type").and_then(Value::as_str) != Some("MCP") {
        return None;
    }
    let output = raw_output.get("output")?;
    if let Some(text) = output.as_str() {
        return (!text.is_empty()).then(|| text.to_string());
    }
    output
        .as_object()?
        .values()
        .find_map(|value| value.as_str().filter(|text| !text.is_empty()))
        .map(str::to_string)
}

pub(super) fn update_tool_output(update: &Value) -> Option<String> {
    if let Some(items) = update.get("content").and_then(Value::as_array) {
        let mut text = String::new();
        for item in items {
            if let Some(chunk) = item
                .get("content")
                .and_then(|content| content.get("text"))
                .and_then(Value::as_str)
            {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(chunk);
            }
        }
        if !text.is_empty() {
            return Some(truncate_str(&text, GROK_TOOL_OUTPUT_CAP));
        }
    }
    if let Some(text) = update
        .get("rawOutput")
        .and_then(|output| output.get("output_for_prompt"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        return Some(truncate_str(text, GROK_TOOL_OUTPUT_CAP));
    }
    update
        .get("rawOutput")
        .and_then(grok_mcp_output_text)
        .map(|text| truncate_str(&text, GROK_TOOL_OUTPUT_CAP))
}

pub(super) fn tool_input_preview(raw: Option<&Value>) -> Option<String> {
    let raw = raw?;
    if raw.is_null() {
        return None;
    }
    serde_json::to_string(raw)
        .ok()
        .map(|value| truncate_str(&value, GROK_TOOL_INPUT_CAP))
}

pub(super) fn grok_mcp_input_preview(input: &Value) -> Option<String> {
    if input.is_null() {
        return None;
    }
    let mut per_string = GROK_TOOL_INPUT_CAP;
    loop {
        let serialized = serde_json::to_string(&cap_json_string_values(input, per_string)).ok()?;
        if serialized.len() <= GROK_TOOL_INPUT_CAP || per_string == 0 {
            return Some(serialized);
        }
        per_string /= 2;
    }
}

fn cap_json_string_values(value: &Value, cap: usize) -> Value {
    match value {
        Value::String(text) => Value::String(truncate_str(text, cap)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| cap_json_string_values(item, cap))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), cap_json_string_values(value, cap)))
                .collect(),
        ),
        other => other.clone(),
    }
}
