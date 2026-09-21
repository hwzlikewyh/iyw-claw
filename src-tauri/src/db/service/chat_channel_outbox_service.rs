use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

use crate::db::entities::chat_channel_outbox;
use crate::db::error::DbError;

pub const MAX_WAITING_PER_RECIPIENT: u64 = 50;

pub async fn enqueue(
    db: &DatabaseConnection,
    channel_id: i32,
    recipient_id: &str,
    content: &str,
) -> Result<chat_channel_outbox::Model, DbError> {
    let waiting = waiting_query(channel_id, recipient_id).count(db).await?;
    if waiting >= MAX_WAITING_PER_RECIPIENT {
        return Err(DbError::Validation(
            "WeChat deferred message queue is full".to_string(),
        ));
    }
    let now = Utc::now();
    Ok(chat_channel_outbox::ActiveModel {
        id: NotSet,
        channel_id: Set(channel_id),
        recipient_id: Set(recipient_id.to_string()),
        content: Set(content.to_string()),
        state: Set("waiting_context".to_string()),
        attempts: Set(0),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?)
}

pub async fn list_waiting(
    db: &DatabaseConnection,
    channel_id: i32,
    recipient_id: &str,
) -> Result<Vec<chat_channel_outbox::Model>, DbError> {
    Ok(waiting_query(channel_id, recipient_id)
        .order_by_asc(chat_channel_outbox::Column::Id)
        .limit(MAX_WAITING_PER_RECIPIENT)
        .all(db)
        .await?)
}

pub async fn remove(db: &DatabaseConnection, id: i32) -> Result<(), DbError> {
    chat_channel_outbox::Entity::delete_by_id(id)
        .exec(db)
        .await?;
    Ok(())
}

pub async fn record_failure(
    db: &DatabaseConnection,
    item: chat_channel_outbox::Model,
    error: &str,
) -> Result<(), DbError> {
    let mut active = item.into_active_model();
    active.attempts = Set(active.attempts.unwrap().saturating_add(1));
    active.last_error = Set(Some(error.chars().take(256).collect()));
    active.updated_at = Set(Utc::now());
    active.update(db).await?;
    Ok(())
}

fn waiting_query(
    channel_id: i32,
    recipient_id: &str,
) -> sea_orm::Select<chat_channel_outbox::Entity> {
    chat_channel_outbox::Entity::find()
        .filter(chat_channel_outbox::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_outbox::Column::RecipientId.eq(recipient_id))
        .filter(chat_channel_outbox::Column::State.eq("waiting_context"))
}
