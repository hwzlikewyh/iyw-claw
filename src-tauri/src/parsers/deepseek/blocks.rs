use serde_json::Value;

use crate::models::{ContentBlock, TurnUsage};
use crate::parsers::truncate_str;

pub(super) const TOOL_INPUT_CAP: usize = 8_000;

pub(super) fn collect_text_parts(content: Option<&Value>) -> String {
    let mut text = String::new();
    for item in content.and_then(Value::as_array).into_iter().flatten() {
        if item.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        if let Some(part) = item.get("text").and_then(Value::as_str) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(part);
        }
    }
    text
}

pub(super) fn usage_from_step(usage: Option<&Value>) -> Option<TurnUsage> {
    let usage = usage?;
    let get = |name| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    let input_tokens = get("inputTokens");
    let output_tokens = get("outputTokens");
    let cache_read_input_tokens = get("cacheReadTokens");
    (input_tokens != 0 || output_tokens != 0 || cache_read_input_tokens != 0).then_some(TurnUsage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens,
    })
}

pub(super) fn add_usage(left: TurnUsage, right: TurnUsage) -> TurnUsage {
    TurnUsage {
        input_tokens: left.input_tokens.saturating_add(right.input_tokens),
        output_tokens: left.output_tokens.saturating_add(right.output_tokens),
        cache_creation_input_tokens: left
            .cache_creation_input_tokens
            .saturating_add(right.cache_creation_input_tokens),
        cache_read_input_tokens: left
            .cache_read_input_tokens
            .saturating_add(right.cache_read_input_tokens),
    }
}

pub(super) fn assistant_block(block: &Value) -> Option<(ContentBlock, bool)> {
    match block.get("type").and_then(Value::as_str)? {
        "text" => block
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                (
                    ContentBlock::Text {
                        text: text.to_string(),
                    },
                    true,
                )
            }),
        "reasoning" => block
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                (
                    ContentBlock::Thinking {
                        text: text.to_string(),
                    },
                    false,
                )
            }),
        "tool-call" => Some((
            ContentBlock::ToolUse {
                tool_use_id: block.get("id").and_then(Value::as_str).map(String::from),
                tool_name: block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                input_preview: block
                    .get("arguments")
                    .and_then(Value::as_str)
                    .and_then(tool_input_preview),
                meta: None,
            },
            false,
        )),
        _ => None,
    }
}

pub(super) fn tool_result_block(data: &Value) -> ContentBlock {
    let result = data
        .pointer("/message/content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("tool-result"))
        });
    let tool_use_id = result
        .and_then(|item| item.get("toolCallId"))
        .and_then(Value::as_str)
        .or_else(|| {
            data.pointer("/message/source/callId")
                .and_then(Value::as_str)
        })
        .map(String::from);
    let output = result.map(|item| collect_text_parts(item.get("content")));
    let is_error = result
        .and_then(|item| item.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || data.get("error").is_some_and(|error| !error.is_null());
    ContentBlock::ToolResult {
        tool_use_id,
        output_preview: output.filter(|text| !text.trim().is_empty()),
        is_error,
        agent_stats: None,
        images: Vec::new(),
    }
}

fn tool_input_preview(arguments: &str) -> Option<String> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() <= TOOL_INPUT_CAP {
        return Some(trimmed.to_string());
    }
    serde_json::from_str(trimmed)
        .ok()
        .and_then(|value| cap_json_to_budget(&value, TOOL_INPUT_CAP))
        .or_else(|| Some(truncate_str(trimmed, TOOL_INPUT_CAP)))
}

fn cap_json_to_budget(value: &Value, budget: usize) -> Option<String> {
    let mut per_string = budget;
    loop {
        let serialized = serde_json::to_string(&cap_json_string_values(value, per_string)).ok()?;
        if serialized.len() <= budget || per_string == 0 {
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
                .map(|(key, item)| (key.clone(), cap_json_string_values(item, cap)))
                .collect(),
        ),
        other => other.clone(),
    }
}
