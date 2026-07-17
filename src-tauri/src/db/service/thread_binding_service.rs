use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, Set,
};

use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::entities::chat_channel_thread_binding;
use crate::db::error::DbError;

pub struct ThreadBindingUpsert<'a> {
    pub target: &'a ChannelMessageTarget,
    pub channel_type: &'a str,
    pub conversation_id: i32,
    pub connection_id: Option<String>,
    pub created_by_sender_id: &'a str,
    pub display_title: Option<String>,
}

struct ThreadIdentity<'a> {
    chat_id: &'a str,
    thread_key: &'a str,
    thread_kind: &'a str,
}

fn thread_identity(target: &ChannelMessageTarget) -> Result<ThreadIdentity<'_>, DbError> {
    Ok(ThreadIdentity {
        chat_id: target
            .chat_id
            .as_deref()
            .ok_or_else(|| DbError::Validation("thread binding requires chat_id".into()))?,
        thread_key: target
            .thread_key
            .as_deref()
            .ok_or_else(|| DbError::Validation("thread binding requires thread_key".into()))?,
        thread_kind: target
            .thread_kind
            .as_deref()
            .ok_or_else(|| DbError::Validation("thread binding requires thread_kind".into()))?,
    })
}

pub async fn get_by_target(
    conn: &DatabaseConnection,
    target: &ChannelMessageTarget,
) -> Result<Option<chat_channel_thread_binding::Model>, DbError> {
    let Ok(identity) = thread_identity(target) else {
        return Ok(None);
    };
    Ok(chat_channel_thread_binding::Entity::find()
        .filter(chat_channel_thread_binding::Column::ChannelId.eq(target.channel_id))
        .filter(chat_channel_thread_binding::Column::ThreadKind.eq(identity.thread_kind))
        .filter(chat_channel_thread_binding::Column::ChatId.eq(identity.chat_id))
        .filter(chat_channel_thread_binding::Column::ThreadKey.eq(identity.thread_key))
        .one(conn)
        .await?)
}

pub async fn list_by_conversation(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<chat_channel_thread_binding::Model>, DbError> {
    Ok(chat_channel_thread_binding::Entity::find()
        .filter(chat_channel_thread_binding::Column::ConversationId.eq(conversation_id))
        .all(conn)
        .await?)
}

pub async fn upsert_for_target(
    conn: &DatabaseConnection,
    input: ThreadBindingUpsert<'_>,
) -> Result<chat_channel_thread_binding::Model, DbError> {
    let identity = thread_identity(input.target)?;
    if let Some(existing) = get_by_target(conn, input.target).await? {
        return update_existing(conn, existing, input).await;
    }
    insert_binding(conn, identity, input).await
}

async fn update_existing(
    conn: &DatabaseConnection,
    existing: chat_channel_thread_binding::Model,
    input: ThreadBindingUpsert<'_>,
) -> Result<chat_channel_thread_binding::Model, DbError> {
    let mut active = existing.into_active_model();
    active.channel_type = Set(input.channel_type.to_string());
    active.conversation_id = Set(input.conversation_id);
    active.connection_id = Set(input.connection_id);
    active.created_by_sender_id = Set(input.created_by_sender_id.to_string());
    active.display_title = Set(input.display_title);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

async fn insert_binding(
    conn: &DatabaseConnection,
    identity: ThreadIdentity<'_>,
    input: ThreadBindingUpsert<'_>,
) -> Result<chat_channel_thread_binding::Model, DbError> {
    let now = Utc::now();
    let active = chat_channel_thread_binding::ActiveModel {
        id: NotSet,
        channel_id: Set(input.target.channel_id),
        channel_type: Set(input.channel_type.to_string()),
        chat_id: Set(identity.chat_id.to_string()),
        thread_key: Set(identity.thread_key.to_string()),
        thread_kind: Set(identity.thread_kind.to_string()),
        conversation_id: Set(input.conversation_id),
        connection_id: Set(input.connection_id),
        created_by_sender_id: Set(input.created_by_sender_id.to_string()),
        display_title: Set(input.display_title),
        title_sync_enabled: Set(true),
        provider_payload_json: Set(input
            .target
            .provider_payload
            .as_ref()
            .map(ToString::to_string)),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(active.insert(conn).await?)
}

pub async fn update_display_title(
    conn: &DatabaseConnection,
    id: i32,
    display_title: String,
) -> Result<chat_channel_thread_binding::Model, DbError> {
    let model = require_binding(conn, id).await?;
    let mut active = model.into_active_model();
    active.display_title = Set(Some(display_title));
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

pub async fn clear_connection(
    conn: &DatabaseConnection,
    id: i32,
) -> Result<chat_channel_thread_binding::Model, DbError> {
    let model = require_binding(conn, id).await?;
    let mut active = model.into_active_model();
    active.connection_id = Set(None);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

async fn require_binding(
    conn: &DatabaseConnection,
    id: i32,
) -> Result<chat_channel_thread_binding::Model, DbError> {
    chat_channel_thread_binding::Entity::find_by_id(id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("thread binding {id}")))
}
