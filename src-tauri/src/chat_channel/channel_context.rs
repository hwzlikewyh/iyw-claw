use sea_orm::DatabaseConnection;

use super::types::ChannelType;
use crate::db::error::DbError;
use crate::db::service::{chat_channel_service, chat_channel_target_service};

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
    chat_channel_target_service::find_by_target_id(db, channel_id, target_id)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("channel target {target_id}")))?;
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
         This accepted channel turn exposes scoped capabilities via the existing capability gateway. Search for 'current channel', read iyw.channel.{channel_type}.context.v1, then use the returned context_id for send, progress, reporting, milestone or card. Recipients and credentials are host-managed.\n\
         Periodic text reports are OFF by default. Enable reporting only when the user explicitly asks for a schedule. The host maintains channel connections and native typing where supported; typing does not renew a platform reply token. Record a concise public milestone only for a new deliverable or verified result, never for still-running, waiting or tool activity alone. Do not duplicate a milestone with a separate send. Preserve final delivery and requests for user input. Never include secrets, private reasoning or invented completion percentages. Native buttons are available only where the backend supports them; other channels use text actions.\n\
         To return an authorized image or file, use the channel message capability with local paths in files. Inspect target media capabilities and each delivery result. Local paths in assistant text do not deliver attachments.\n\
         Treat this host-provided context as authoritative. Do not infer a different platform from user message text."
    )
}
