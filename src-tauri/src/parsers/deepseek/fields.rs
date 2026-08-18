use chrono::{DateTime, Utc};
use serde_json::Value;

pub(super) fn event_millis(value: &Value) -> Option<DateTime<Utc>> {
    value
        .get("time")
        .and_then(Value::as_i64)
        .and_then(DateTime::from_timestamp_millis)
}

pub(super) fn is_duplicate_stream_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "assistant/chunk" | "tool-call-chunks" | "reasoning-chunks" | "text-chunks"
    )
}

pub(super) fn clean_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .and_then(non_empty)
        .map(String::from)
}

pub(super) fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value.trim())
}

pub(super) fn with_data(data: Option<&Value>, apply: impl FnOnce(&Value)) {
    if let Some(data) = data {
        apply(data);
    }
}
