use sea_orm::DatabaseConnection;

use super::types::ChannelMessageTarget;
use crate::db::entities::{chat_channel, chat_channel_target};
use crate::db::error::DbError;
use crate::db::service::chat_channel_target_service;

pub async fn register_default(
    db: &DatabaseConnection,
    channel: &chat_channel::Model,
) -> Result<Option<chat_channel_target::Model>, DbError> {
    let config: serde_json::Value = serde_json::from_str(&channel.config_json)
        .map_err(|error| DbError::Validation(format!("invalid channel config: {error}")))?;
    let target = match channel.channel_type.as_str() {
        "lark" => string_field(&config, "chat_id").map(|chat_id| ChannelMessageTarget {
            channel_id: channel.id,
            chat_id: Some(chat_id),
            thread_key: None,
            thread_kind: Some("lark_chat".to_string()),
            provider_payload: None,
        }),
        "wecom" => string_field(&config, "default_chatid").map(|chat_id| {
            let chat_type = config
                .get("default_chat_type")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            ChannelMessageTarget {
                channel_id: channel.id,
                chat_id: Some(chat_id),
                thread_key: None,
                thread_kind: Some("wecom_chat".to_string()),
                provider_payload: Some(serde_json::json!({ "chat_type": chat_type })),
            }
        }),
        "wecom_ai_bot" => {
            string_field(&config, "default_chatid").map(|chat_id| ChannelMessageTarget {
                channel_id: channel.id,
                chat_id: Some(chat_id),
                thread_key: None,
                thread_kind: Some("wecom_ai_bot".to_string()),
                provider_payload: None,
            })
        }
        _ => None,
    };
    let Some(target) = target else {
        chat_channel_target_service::clear_default_target(db, channel.id).await?;
        return Ok(None);
    };
    let display_name = format!("{} 默认会话", channel.name);
    let saved = chat_channel_target_service::upsert(
        db,
        &target,
        chat_channel_target_service::TargetMetadata {
            display_name: &display_name,
            target_kind: "chat",
            source: chat_channel_target_service::SOURCE_DEFAULT,
            is_default: true,
        },
    )
    .await?;
    Ok(Some(saved))
}

pub async fn register_inbound(
    db: &DatabaseConnection,
    target: &ChannelMessageTarget,
    display_name: &str,
) -> Result<chat_channel_target::Model, DbError> {
    chat_channel_target_service::upsert(
        db,
        target,
        chat_channel_target_service::TargetMetadata {
            display_name,
            target_kind: target_kind(target),
            source: chat_channel_target_service::SOURCE_INBOUND,
            is_default: false,
        },
    )
    .await
}

pub async fn resolve_default(
    db: &DatabaseConnection,
    channel_id: i32,
) -> Result<Option<(chat_channel_target::Model, ChannelMessageTarget)>, DbError> {
    let target = chat_channel_target_service::list_by_channel(db, channel_id)
        .await?
        .into_iter()
        .find(|target| target.is_default);
    let Some(target) = target else {
        return Ok(None);
    };
    chat_channel_target_service::resolve(db, channel_id, &target.target_id)
        .await
        .map(Some)
}

fn target_kind(target: &ChannelMessageTarget) -> &'static str {
    match target.thread_kind.as_deref() {
        Some("wecom_chat") => "wecom_chat",
        Some("wecom_ai_bot") => "wecom_ai_bot",
        Some("dingtalk_chat") => "dingtalk_chat",
        Some("lark_chat") => "lark_chat",
        Some("weixin_context") => "weixin_context",
        Some(_) => "thread",
        None => "channel",
    }
}

fn string_field(config: &serde_json::Value, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
