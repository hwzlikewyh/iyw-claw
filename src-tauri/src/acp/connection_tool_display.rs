use std::sync::Arc;

use serde_json::{Map, Value};
use tokio::sync::RwLock;

use crate::acp::builtin_mcp::BuiltinMcpClient;
use crate::acp::session_state::SessionState;

const META_KEY: &str = "iyw.toolDisplay";
const MAX_INPUT_DEPTH: usize = 4;
const INPUT_KEYS: [&str; 4] = ["input", "arguments", "params", "payload"];

pub(super) struct ToolDisplayInput<'a> {
    pub state: &'a Arc<RwLock<SessionState>>,
    pub tool_call_id: &'a str,
    pub raw_input: Option<&'a str>,
    pub client: Option<&'a BuiltinMcpClient>,
}

pub(super) async fn enrich(meta: Option<Value>, input: ToolDisplayInput<'_>) -> Option<Value> {
    let raw_input = input.raw_input.map(|value| Value::String(value.to_owned()));
    let (id, previous_display) = {
        let state = input.state.read().await;
        let previous = state.active_tool_calls.get(input.tool_call_id);
        (
            raw_input
                .as_ref()
                .or_else(|| previous?.input.as_ref())
                .and_then(|value| capability_id(value, 0)),
            previous
                .and_then(|tool| tool.meta.as_ref()?.get(META_KEY))
                .cloned(),
        )
    };
    let cached = match (input.client, id.as_deref()) {
        (Some(client), Some(id)) => client.remote_tool_display(id).await,
        _ => None,
    };
    let display = cached.or_else(|| {
        previous_display.filter(|display| {
            id.as_deref()
                .is_some_and(|id| display["capability_id"].as_str() == Some(id))
        })
    });
    let Some(display) = display else { return meta };
    let mut meta = meta
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    meta.insert(META_KEY.to_owned(), display);
    Some(Value::Object(meta))
}

fn capability_id(value: &Value, depth: usize) -> Option<String> {
    if depth > MAX_INPUT_DEPTH {
        return None;
    }
    match value {
        Value::String(text) if text.contains("iyw.remote.") => {
            capability_id(&serde_json::from_str::<Value>(text).ok()?, depth + 1)
        }
        Value::Array(items) => items.iter().find_map(|item| capability_id(item, depth + 1)),
        Value::Object(object) => match object.get("capability_id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => id
                .trim()
                .starts_with("iyw.remote.")
                .then(|| id.trim().to_owned()),
            _ => nested_capability_id(object, depth),
        },
        _ => None,
    }
}

fn nested_capability_id(object: &Map<String, Value>, depth: usize) -> Option<String> {
    INPUT_KEYS
        .iter()
        .find_map(|key| capability_id(object.get(*key)?, depth + 1))
}
