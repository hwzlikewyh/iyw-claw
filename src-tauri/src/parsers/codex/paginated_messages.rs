use std::collections::HashSet;

use serde_json::{json, Value};

/// 新版分页历史保存 TurnItem，旧版 user/agent 事件只存在于 legacy 历史。
/// 按官方 protocol/legacy_events.rs 映射到现有解析分支，摘要与详情共用。
#[derive(Default)]
pub(super) struct PaginatedMessages {
    command_descriptions: super::command_descriptions::DescribedCommands,
    enabled: bool,
    seen_items: HashSet<(String, String)>,
    pending_images: Vec<String>,
}

impl PaginatedMessages {
    pub(super) fn normalize(&mut self, record: &mut Value) -> bool {
        if !self.prepare_record(record) {
            return false;
        }
        self.command_descriptions.normalize(record);
        if !self.enabled
            || record.get("type").and_then(Value::as_str) != Some("event_msg")
            || record.pointer("/payload/type").and_then(Value::as_str) != Some("item_completed")
        {
            return true;
        }
        let payload = &record["payload"];
        let Some(normalized) = self.completed_message(payload) else {
            return true;
        };
        let Some(key) = item_key(payload) else {
            tracing::warn!("[codex-history] completed message is missing turn/item identity");
            return false;
        };
        if !self.seen_items.insert(key) {
            return false;
        }
        record["payload"] = normalized;
        true
    }

    fn prepare_record(&mut self, record: &Value) -> bool {
        let kind = record.get("type").and_then(Value::as_str).unwrap_or("");
        let Some(payload) = record.get("payload") else {
            return true;
        };
        if kind == "session_meta" {
            self.enabled = payload.get("history_mode").and_then(Value::as_str) == Some("paginated");
            tracing::debug!(
                paginated = self.enabled,
                "[codex-history] selected message event format"
            );
        }
        if !self.enabled {
            return true;
        }
        let payload_type = payload.get("type").and_then(Value::as_str);
        if kind == "turn_context" || (kind == "event_msg" && payload_type == Some("task_started")) {
            self.pending_images.clear();
        }
        if kind == "response_item" && payload_type == Some("message") {
            if payload.get("role").and_then(Value::as_str) == Some("user") {
                self.pending_images = response_images(payload);
            }
            // 正文只从有明确消息身份的完成事件读取，避免内部上下文及双份正文。
            return false;
        }
        true
    }

    fn completed_message(&mut self, payload: &Value) -> Option<Value> {
        let item = payload.get("item")?;
        match item.get("type")?.as_str()? {
            "UserMessage" => user_message(item, std::mem::take(&mut self.pending_images)),
            "AgentMessage" => {
                let message = text_content(item, "Text", "\n\n");
                (!message.is_empty()).then(|| {
                    json!({
                        "type": "agent_message",
                        "message": message,
                        "phase": item.get("phase"),
                    })
                })
            }
            _ => None,
        }
    }
}

fn item_key(payload: &Value) -> Option<(String, String)> {
    Some((
        payload.get("turn_id")?.as_str()?.to_string(),
        payload.get("item")?.get("id")?.as_str()?.to_string(),
    ))
}

fn text_content(item: &Value, kind: &str, separator: &str) -> String {
    item.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some(kind))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(separator)
}

fn response_images(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("input_image"))
        .filter_map(super::input_image_url)
        .map(str::to_owned)
        .collect()
}

fn user_message(item: &Value, mut images: Vec<String>) -> Option<Value> {
    // 上游 UserMessageItem::message() 按块直接拼接；先去掉宿主记忆上下文，
    // 防止它抢占首条真实输入的标题，同时保留封装之后的用户原文。
    let message = crate::user_memory::strip_user_context(&text_content(item, "text", ""));
    if images.is_empty() {
        images = item
            .get("content")?
            .as_array()?
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("image"))
            .filter_map(|part| part.get("image_url").and_then(Value::as_str))
            .map(str::to_owned)
            .collect();
    }
    if message.trim().is_empty() && images.is_empty() {
        return None;
    }
    Some(json!({
        "type": "user_message",
        "message": message,
        "images": images,
        // 新格式已按 turn/item ID 去重，不能再按内容删除真实的重复提问。
        "history_item_id": item.get("id"),
    }))
}
