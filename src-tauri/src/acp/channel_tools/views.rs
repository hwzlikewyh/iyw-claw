use serde::Serialize;
use serde_json::{json, Value};

use crate::db::entities::{chat_channel, chat_channel_message_log, chat_channel_target};
use crate::db::service::chat_channel_target_service;

const MESSAGE_PREVIEW_CHARS: usize = 500;

#[derive(Debug, Serialize)]
pub struct ChannelView {
    pub channel_id: i32,
    pub name: String,
    pub channel_type: String,
    pub enabled: bool,
    pub runtime_status: String,
    pub last_error: Option<String>,
    pub last_connected_at: Option<String>,
    pub daily_report_enabled: bool,
    pub daily_report_time: Option<String>,
    pub credential_configured: bool,
    pub default_target: Option<DefaultTargetView>,
    pub available_operations: Vec<&'static str>,
    pub config: Value,
}

impl ChannelView {
    pub async fn from_model(
        db: &sea_orm::DatabaseConnection,
        model: chat_channel::Model,
        wecom_authorized: Option<bool>,
    ) -> Result<Self, String> {
        let config = safe_config(&model);
        let credential_configured = credential_status(&model, wecom_authorized);
        let default_target = default_target(db, model.id).await?;
        Ok(Self {
            channel_id: model.id,
            name: safe_text(&model.name, 128),
            channel_type: model.channel_type,
            enabled: model.enabled,
            runtime_status: model.runtime_status,
            last_error: model.last_error.as_deref().map(safe_error),
            last_connected_at: model.last_connected_at.map(|value| value.to_rfc3339()),
            daily_report_enabled: model.daily_report_enabled,
            daily_report_time: model.daily_report_time,
            credential_configured,
            default_target,
            available_operations: vec![
                "update",
                "delete",
                "manage_credential",
                "connect",
                "disconnect",
                "diagnose",
                "send",
            ],
            config,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct DefaultTargetView {
    pub target_id: String,
    pub target_kind: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct TargetView {
    pub target_id: String,
    pub target_kind: String,
    pub source: String,
    pub display_name: String,
    pub is_default: bool,
    pub last_interaction_at: String,
    pub capabilities: Value,
}

impl TargetView {
    pub fn from_model(model: chat_channel_target::Model, channel_type: &str) -> Self {
        Self {
            target_id: model.target_id,
            target_kind: model.target_kind,
            source: model.source,
            display_name: safe_text(&model.display_name, 128),
            is_default: model.is_default,
            last_interaction_at: model.last_seen_at.to_rfc3339(),
            capabilities: capabilities(channel_type),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MessageView {
    pub message_id: String,
    pub channel_id: i32,
    pub target_id: String,
    pub direction: String,
    pub message_type: String,
    pub content_preview: String,
    pub status: String,
    pub created_at: String,
}

impl From<chat_channel_message_log::Model> for MessageView {
    fn from(model: chat_channel_message_log::Model) -> Self {
        Self {
            message_id: format!("cm_{}", model.id),
            channel_id: model.channel_id,
            target_id: model
                .target_id
                .unwrap_or_else(|| "legacy_unknown".to_string()),
            direction: model.direction,
            message_type: model.message_type,
            content_preview: safe_preview(&model.content_preview),
            status: model.status,
            created_at: model.created_at.to_rfc3339(),
        }
    }
}

pub fn safe_error(_error: &str) -> String {
    "渠道操作未完成，请在 iyw-claw 设置中查看详情".to_string()
}

pub fn safe_preview(value: &str) -> String {
    let single_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    safe_text(&redact_tokens(&single_line), MESSAGE_PREVIEW_CHARS)
}

pub fn safe_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn credential_status(model: &chat_channel::Model, wecom_authorized: Option<bool>) -> bool {
    if model.channel_type == "wecom" {
        wecom_authorized.unwrap_or(false)
    } else {
        crate::keyring_store::get_channel_token(model.id).is_some()
    }
}

async fn default_target(
    db: &sea_orm::DatabaseConnection,
    channel_id: i32,
) -> Result<Option<DefaultTargetView>, String> {
    Ok(chat_channel_target_service::list_by_channel(db, channel_id)
        .await
        .map_err(|_| "TARGET_QUERY_FAILED".to_string())?
        .into_iter()
        .find(|target| target.is_default)
        .map(|target| DefaultTargetView {
            target_id: target.target_id,
            target_kind: target.target_kind,
            display_name: safe_text(&target.display_name, 128),
        }))
}

fn safe_config(model: &chat_channel::Model) -> Value {
    let parsed = serde_json::from_str::<Value>(&model.config_json).unwrap_or(Value::Null);
    let get = |key: &str| parsed.get(key).cloned().unwrap_or(Value::Null);
    json!({
        "app_id_configured": parsed.get("app_id").and_then(Value::as_str).is_some_and(|v| !v.is_empty()),
        "bot_id_configured": parsed.get("bot_id").and_then(Value::as_str).is_some_and(|v| !v.is_empty()),
        "client_id_configured": parsed.get("client_id").and_then(Value::as_str).is_some_and(|v| !v.is_empty()),
        "default_target_configured": parsed.get("chat_id").or_else(|| parsed.get("default_chatid")).and_then(Value::as_str).is_some_and(|v| !v.is_empty()),
        "default_target_type": get("default_chat_type"),
        "default_agent_type": get("default_agent_type"),
        "poll_interval_secs": get("poll_interval_secs"),
        "base_url_configured": parsed.get("base_url").and_then(Value::as_str).is_some_and(|v| !v.is_empty()),
    })
}

fn capabilities(channel_type: &str) -> Value {
    json!({
        "text": true,
        "rich_text": true,
        "attachments": channel_type == "lark",
        "max_file_bytes": (channel_type == "lark").then_some(30 * 1024 * 1024),
    })
}

fn redact_tokens(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.contains("token=") || lower.contains("key=") || lower.contains("secret=") {
                "[redacted]"
            } else if lower.starts_with("http://") || lower.starts_with("https://") {
                "[url]"
            } else if std::path::Path::new(part).is_absolute() {
                std::path::Path::new(part)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("[file]")
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
