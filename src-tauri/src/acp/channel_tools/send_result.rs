use serde_json::{json, Value};

use super::send_files::FileSendResult;
use super::types::{RichMessageInput, SendItemInput};
use crate::chat_channel::types::{MessageLevel, RichMessage};

pub(super) fn build_message(item: &SendItemInput) -> Result<Option<RichMessage>, String> {
    if item.text.is_some() && item.rich.is_some() {
        return Err("MESSAGE_CONTENT_CONFLICT".to_string());
    }
    if let Some(text) = item.text.as_deref() {
        if text.trim().is_empty() {
            return Err("MESSAGE_CONTENT_REQUIRED".to_string());
        }
        return Ok(Some(RichMessage::info(text)));
    }
    item.rich.as_ref().map(rich_message).transpose()
}

fn rich_message(input: &RichMessageInput) -> Result<RichMessage, String> {
    if input.body.trim().is_empty() {
        return Err("MESSAGE_CONTENT_REQUIRED".to_string());
    }
    let level = match input.level.as_deref().unwrap_or("info") {
        "info" => MessageLevel::Info,
        "warning" => MessageLevel::Warning,
        "error" => MessageLevel::Error,
        _ => return Err("INVALID_MESSAGE_LEVEL".to_string()),
    };
    Ok(RichMessage {
        title: input.title.clone(),
        body: input.body.clone(),
        fields: input
            .fields
            .iter()
            .map(|field| (field.label.clone(), field.value.clone()))
            .collect(),
        level,
    })
}

pub(super) fn map_send_error(error: crate::chat_channel::error::ChatChannelError) -> String {
    let value = error.to_string();
    if value.contains("TARGET_CONTEXT_EXPIRED") {
        "TARGET_CONTEXT_EXPIRED".to_string()
    } else if matches!(
        error,
        crate::chat_channel::error::ChatChannelError::NotConnected
            | crate::chat_channel::error::ChatChannelError::NotFound(_)
    ) {
        "CHANNEL_NOT_CONNECTED".to_string()
    } else {
        "CHANNEL_SEND_FAILED".to_string()
    }
}

pub(super) fn with_index(mut value: Value, index: usize) -> Value {
    add_field(&mut value, "index", json!(index));
    value
}

pub(super) fn add_field(value: &mut Value, name: &str, field: Value) {
    if let Some(object) = value.as_object_mut() {
        object.insert(name.to_string(), field);
    }
}

pub(super) fn result_index(value: &Value) -> u64 {
    value
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX)
}

pub(super) fn batch_status(items: &[Value]) -> &'static str {
    if items
        .iter()
        .all(|item| item.get("status").and_then(Value::as_str) == Some("sent"))
    {
        return "success";
    }
    let delivered = items
        .iter()
        .filter(|item| {
            matches!(
                item.get("status").and_then(Value::as_str),
                Some("sent" | "partial_success")
            )
        })
        .count();
    if delivered == 0 {
        "failed"
    } else {
        "partial_success"
    }
}

pub(super) fn send_status(
    text_requested: bool,
    text_sent: bool,
    files: &[FileSendResult],
    files_empty: bool,
) -> Result<&'static str, String> {
    let file_sent = files.iter().filter(|file| file.status == "sent").count();
    let successes = usize::from(text_sent) + file_sent;
    let failures = files.len().saturating_sub(file_sent);
    match (successes, failures, files_empty, text_requested) {
        (0, 0, true, false) => Err("MESSAGE_CONTENT_REQUIRED".to_string()),
        (0, 0, true, true) => Ok("failed"),
        (0, _, _, _) => Ok("failed"),
        (_, 0, _, _) => Ok("sent"),
        _ => Ok("partial_success"),
    }
}

pub(super) fn first_file_error(files: &[FileSendResult]) -> Option<&'static str> {
    files.iter().find_map(|file| file.error)
}
