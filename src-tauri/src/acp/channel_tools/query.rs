use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::service::ChannelToolService;
use super::types::{ListMessagesInput, ListTargetsInput};
use super::views::{MessageView, TargetView};
use crate::db::service::{
    chat_channel_message_log_service, chat_channel_service, chat_channel_target_service,
};

const DEFAULT_MESSAGE_LIMIT: u64 = 50;
const MAX_MESSAGE_LIMIT: u64 = 100;

#[derive(Serialize, Deserialize)]
struct MessageCursor {
    created_at: DateTime<Utc>,
    id: i32,
}

impl ChannelToolService {
    pub(super) async fn list_targets(
        &self,
        input: Result<ListTargetsInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        let channel = chat_channel_service::get_by_id(&self.db.conn, input.channel_id)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
        let since = input.since.as_deref().map(parse_time).transpose()?;
        let rows = chat_channel_target_service::list_by_channel(&self.db.conn, input.channel_id)
            .await
            .map_err(|_| "TARGET_QUERY_FAILED".to_string())?;
        let targets = rows
            .into_iter()
            .filter(|row| {
                row.is_default || row.source == chat_channel_target_service::SOURCE_INBOUND
            })
            .filter(|row| {
                input
                    .target_kind
                    .as_deref()
                    .is_none_or(|kind| row.target_kind == kind)
            })
            .filter(|row| since.is_none_or(|time| row.last_seen_at >= time))
            .map(|row| TargetView::from_model(row, &channel.channel_type))
            .collect::<Vec<_>>();
        Ok(json!({ "targets": targets }))
    }

    pub(super) async fn list_messages(
        &self,
        input: Result<ListMessagesInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        ensure_channel(&self.db, input.channel_id).await?;
        let limit = input
            .limit
            .unwrap_or(DEFAULT_MESSAGE_LIMIT)
            .clamp(1, MAX_MESSAGE_LIMIT);
        validate_direction(input.direction.as_deref())?;
        validate_status(input.status.as_deref())?;
        let since = input.since.as_deref().map(parse_time).transpose()?;
        let until = input.until.as_deref().map(parse_time).transpose()?;
        if since.zip(until).is_some_and(|(start, end)| start > end) {
            return Err("INVALID_TIME_RANGE".to_string());
        }
        let cursor = parse_cursor(input.cursor.as_deref())?;
        let mut rows = chat_channel_message_log_service::list_filtered(
            &self.db.conn,
            input.channel_id,
            input.target_id.as_deref(),
            input.direction.as_deref(),
            input.status.as_deref(),
            since,
            until,
            cursor.map(|value| (value.created_at, value.id)),
            limit.saturating_add(1),
        )
        .await
        .map_err(|_| "MESSAGE_QUERY_FAILED".to_string())?;
        let has_more = rows.len() as u64 > limit;
        if has_more {
            rows.truncate(limit as usize);
        }
        let next_cursor = has_more
            .then(|| rows.last())
            .flatten()
            .map(encode_cursor)
            .transpose()?;
        let messages = rows.into_iter().map(MessageView::from).collect::<Vec<_>>();
        Ok(json!({ "messages": messages, "next_cursor": next_cursor }))
    }
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| "INVALID_SINCE".to_string())
}

fn parse_cursor(value: Option<&str>) -> Result<Option<MessageCursor>, String> {
    let Some(value) = value else { return Ok(None) };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| "INVALID_CURSOR".to_string())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "INVALID_CURSOR".to_string())
}

fn encode_cursor(
    row: &crate::db::entities::chat_channel_message_log::Model,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(&MessageCursor {
        created_at: row.created_at,
        id: row.id,
    })
    .map_err(|_| "CURSOR_ENCODING_FAILED".to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn validate_direction(value: Option<&str>) -> Result<(), String> {
    match value {
        None | Some("inbound" | "outbound") => Ok(()),
        Some(_) => Err("INVALID_DIRECTION".to_string()),
    }
}

fn validate_status(value: Option<&str>) -> Result<(), String> {
    match value {
        None | Some("sent" | "failed") => Ok(()),
        Some(_) => Err("INVALID_STATUS".to_string()),
    }
}

async fn ensure_channel(db: &crate::db::AppDatabase, channel_id: i32) -> Result<(), String> {
    chat_channel_service::get_by_id(&db.conn, channel_id)
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
        .map(|_| ())
        .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())
}
