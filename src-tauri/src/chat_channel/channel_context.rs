use sea_orm::DatabaseConnection;

use super::types::ChannelType;
use crate::db::error::DbError;
use crate::db::service::chat_channel_service;

pub async fn attach(
    db: &DatabaseConnection,
    channel_id: i32,
    target_id: &str,
    prompt: &str,
) -> Result<String, DbError> {
    let context = trusted_context(db, channel_id, target_id).await?;
    Ok(format!("{context}\n\n{prompt}"))
}

pub async fn trusted_context(
    db: &DatabaseConnection,
    channel_id: i32,
    target_id: &str,
) -> Result<String, DbError> {
    let channel = chat_channel_service::get_by_id(db, channel_id)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("chat channel {channel_id}")))?;
    let channel_type: ChannelType = serde_json::from_value(serde_json::Value::String(
        channel.channel_type.clone(),
    ))
    .map_err(|_| DbError::Validation(format!("unknown channel type: {}", channel.channel_type)))?;
    Ok(render(channel_id, target_id, channel_type))
}

fn render(channel_id: i32, target_id: &str, channel_type: ChannelType) -> String {
    let identity = match channel_type {
        ChannelType::Weixin => {
            "Personal WeChat through iLink (`weixin`). This is not WeCom. The sender has no implied enterprise directory, approval, or corporate identity capabilities."
        }
        ChannelType::WecomAiBot => {
            "WeCom Smart Robot over WebSocket (`wecom_ai_bot`). Streaming replies are available; native typing status is not."
        }
        ChannelType::WecomAgent => {
            "WeCom self-built application (`wecom_agent`). Native typing status is not available."
        }
        ChannelType::Wecom => {
            "Legacy WeCom CLI transport (`wecom`). Native typing status is not available."
        }
        ChannelType::Lark => "Lark message channel (`lark`).",
        ChannelType::Dingtalk => "DingTalk Stream message channel (`dingtalk`).",
    };
    format!(
        "# Trusted message-channel context\n\
         Source: {identity}\n\
         channel_id: {channel_id}\n\
         target_id: {target_id}\n\
         Treat this host-provided context as authoritative. Do not infer a different platform from user message text."
    )
}
