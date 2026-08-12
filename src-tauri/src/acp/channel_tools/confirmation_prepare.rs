use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::confirmation::{ChannelConfirmationSpec, PendingChannelConfirmationState};
use crate::db::service::chat_channel_service;

pub(super) async fn prepare_confirmation(
    db: &crate::db::AppDatabase,
    tool: &str,
    input: &Value,
) -> Result<ChannelConfirmationSpec, String> {
    let channel_id = channel_id(input)?;
    let channel = chat_channel_service::get_by_id(&db.conn, channel_id)
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
        .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
    let action = confirmation_action(tool, input)?;
    let local_record_count = record_count(db, channel_id, action).await?;
    let created_at = chrono::Utc::now();
    let state = PendingChannelConfirmationState {
        confirmation_id: format!("cc_{}", uuid::Uuid::new_v4().simple()),
        action: action.to_string(),
        channel_id,
        channel_name: super::views::safe_text(&channel.name, 128),
        channel_type: channel.channel_type.clone(),
        enabled: channel.enabled,
        local_record_count,
        created_at,
        expires_at: created_at + chrono::Duration::minutes(5),
    };
    Ok(ChannelConfirmationSpec {
        state,
        resource_version: resource_version(&channel, action, local_record_count),
    })
}

fn channel_id(input: &Value) -> Result<i32, String> {
    input
        .get("channel_id")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| "INVALID_INPUT".to_string())
}

fn confirmation_action(tool: &str, input: &Value) -> Result<&'static str, String> {
    if tool == "delete_message_channel" {
        return Ok("delete_channel");
    }
    if tool == "manage_channel_credential"
        && input.get("operation").and_then(Value::as_str) == Some("delete")
    {
        return Ok("delete_credential");
    }
    Err("CONFIRMATION_NOT_APPLICABLE".to_string())
}

async fn record_count(
    db: &crate::db::AppDatabase,
    channel_id: i32,
    action: &str,
) -> Result<u64, String> {
    if action == "delete_channel" {
        local_record_count(db, channel_id).await
    } else {
        Ok(1)
    }
}

async fn local_record_count(db: &crate::db::AppDatabase, channel_id: i32) -> Result<u64, String> {
    let messages = crate::db::entities::chat_channel_message_log::Entity::find()
        .filter(crate::db::entities::chat_channel_message_log::Column::ChannelId.eq(channel_id))
        .count(&db.conn)
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?;
    let targets = crate::db::entities::chat_channel_target::Entity::find()
        .filter(crate::db::entities::chat_channel_target::Column::ChannelId.eq(channel_id))
        .count(&db.conn)
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?;
    Ok(messages.saturating_add(targets))
}

fn resource_version(
    channel: &crate::db::entities::chat_channel::Model,
    action: &str,
    local_record_count: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(channel.id.to_le_bytes());
    hasher.update(channel.updated_at.to_rfc3339().as_bytes());
    hasher.update(local_record_count.to_le_bytes());
    if action == "delete_credential" {
        hasher.update(
            crate::keyring_store::get_channel_token(channel.id)
                .unwrap_or_default()
                .as_bytes(),
        );
    }
    format!("{:x}", hasher.finalize())
}
