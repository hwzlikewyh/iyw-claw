use chrono::Utc;
use sea_orm::prelude::DateTimeUtc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};

use crate::db::entities::chat_channel_message_log;
use crate::db::error::DbError;

#[allow(clippy::too_many_arguments)]
pub async fn create_log(
    conn: &DatabaseConnection,
    channel_id: i32,
    direction: &str,
    message_type: &str,
    content_preview: &str,
    status: &str,
    error_detail: Option<String>,
) -> Result<(), DbError> {
    create_log_full(
        conn,
        channel_id,
        direction,
        message_type,
        content_preview,
        status,
        error_detail,
        None,
        None,
    )
    .await
}

/// Like `create_log` but also stamps the end-to-end `trace_id` and the
/// provider's `provider_message_id` when known.
#[allow(clippy::too_many_arguments)]
pub async fn create_log_full(
    conn: &DatabaseConnection,
    channel_id: i32,
    direction: &str,
    message_type: &str,
    content_preview: &str,
    status: &str,
    error_detail: Option<String>,
    trace_id: Option<String>,
    provider_message_id: Option<String>,
) -> Result<(), DbError> {
    let active = chat_channel_message_log::ActiveModel {
        id: NotSet,
        channel_id: Set(channel_id),
        direction: Set(direction.to_string()),
        message_type: Set(message_type.to_string()),
        content_preview: Set(truncate_preview(content_preview)),
        status: Set(status.to_string()),
        error_detail: Set(error_detail),
        trace_id: Set(trace_id),
        provider_message_id: Set(provider_message_id),
        created_at: Set(Utc::now()),
    };
    active.insert(conn).await?;
    Ok(())
}

pub async fn list_by_channel(
    conn: &DatabaseConnection,
    channel_id: i32,
    limit: u64,
    offset: u64,
) -> Result<Vec<chat_channel_message_log::Model>, DbError> {
    use sea_orm::PaginatorTrait;
    Ok(chat_channel_message_log::Entity::find()
        .filter(chat_channel_message_log::Column::ChannelId.eq(channel_id))
        .order_by_desc(chat_channel_message_log::Column::CreatedAt)
        .paginate(conn, limit)
        .fetch_page(offset / limit)
        .await?)
}

/// Look up outbound rows matching a trace id (used by the full-loop
/// diagnostic to verify a probe made it all the way to an outbound reply).
pub async fn list_by_trace(
    conn: &DatabaseConnection,
    trace_id: &str,
    limit: u64,
) -> Result<Vec<chat_channel_message_log::Model>, DbError> {
    Ok(chat_channel_message_log::Entity::find()
        .filter(chat_channel_message_log::Column::TraceId.eq(trace_id))
        .order_by_asc(chat_channel_message_log::Column::CreatedAt)
        .limit(limit)
        .all(conn)
        .await?)
}

pub async fn cleanup_old_logs(
    conn: &DatabaseConnection,
    older_than: DateTimeUtc,
) -> Result<u64, DbError> {
    let result = chat_channel_message_log::Entity::delete_many()
        .filter(chat_channel_message_log::Column::CreatedAt.lt(older_than))
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}

fn truncate_preview(s: &str) -> String {
    if s.len() <= 200 {
        s.to_string()
    } else {
        let mut end = 200;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}
