use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, Set,
};
mod secure;
use secure::fingerprint;
pub use secure::{restore_secure_targets, take_secure_targets};

use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::entities::chat_channel_target;
use crate::db::error::DbError;

pub const SOURCE_DEFAULT: &str = "default";
pub const SOURCE_INBOUND: &str = "known_conversation";

pub struct TargetMetadata<'a> {
    pub display_name: &'a str,
    pub target_kind: &'a str,
    pub source: &'a str,
    pub is_default: bool,
}

pub async fn upsert(
    conn: &DatabaseConnection,
    target: &ChannelMessageTarget,
    metadata: TargetMetadata<'_>,
) -> Result<chat_channel_target::Model, DbError> {
    let payload = serde_json::to_string(target)
        .map_err(|error| DbError::Validation(format!("invalid channel target: {error}")))?;
    let (fingerprint, existing) = prepare_upsert(conn, target).await?;
    if let Some(existing) = existing {
        let saved = update_existing(
            conn,
            existing,
            TargetUpdate {
                payload: &payload,
                display_name: metadata.display_name,
                target_kind: metadata.target_kind,
                source: metadata.source,
                is_default: metadata.is_default,
            },
        )
        .await?;
        if metadata.is_default {
            clear_other_defaults(conn, target.channel_id, Some(saved.id)).await?;
        }
        return Ok(saved);
    }

    let is_default = metadata.is_default;
    let saved = insert_target(
        conn,
        TargetInsert {
            target,
            payload: &payload,
            fingerprint,
            metadata,
        },
    )
    .await?;
    if is_default {
        if let Err(error) = clear_other_defaults(conn, target.channel_id, Some(saved.id)).await {
            cleanup_inserted(conn, &saved).await;
            return Err(error);
        }
    }
    Ok(saved)
}

async fn prepare_upsert(
    conn: &DatabaseConnection,
    target: &ChannelMessageTarget,
) -> Result<(String, Option<chat_channel_target::Model>), DbError> {
    let fingerprint = fingerprint(target)?;
    let existing = chat_channel_target::Entity::find()
        .filter(chat_channel_target::Column::ChannelId.eq(target.channel_id))
        .filter(chat_channel_target::Column::Fingerprint.eq(&fingerprint))
        .one(conn)
        .await?;
    Ok((fingerprint, existing))
}

async fn cleanup_inserted(conn: &DatabaseConnection, model: &chat_channel_target::Model) {
    if chat_channel_target::Entity::delete_by_id(model.id)
        .exec(conn)
        .await
        .is_ok()
    {
        let _ = crate::keyring_store::delete_channel_target(&model.target_id);
    }
}

struct TargetInsert<'a> {
    target: &'a ChannelMessageTarget,
    payload: &'a str,
    fingerprint: String,
    metadata: TargetMetadata<'a>,
}

async fn insert_target(
    conn: &DatabaseConnection,
    insert: TargetInsert<'_>,
) -> Result<chat_channel_target::Model, DbError> {
    let target_id = format!("ct_{}", uuid::Uuid::new_v4().simple());
    crate::keyring_store::set_channel_target(&target_id, insert.payload)
        .map_err(DbError::Migration)?;
    let now = Utc::now();
    let active = chat_channel_target::ActiveModel {
        id: NotSet,
        channel_id: Set(insert.target.channel_id),
        target_id: Set(target_id.clone()),
        target_kind: Set(insert.metadata.target_kind.to_string()),
        source: Set(insert.metadata.source.to_string()),
        display_name: Set(safe_label(insert.metadata.display_name)),
        fingerprint: Set(insert.fingerprint),
        is_default: Set(insert.metadata.is_default),
        first_seen_at: Set(now),
        last_seen_at: Set(now),
        created_at: Set(now),
        updated_at: Set(now),
    };
    match active.insert(conn).await {
        Ok(model) => Ok(model),
        Err(error) => {
            let _ = crate::keyring_store::delete_channel_target(&target_id);
            Err(error.into())
        }
    }
}

pub async fn list_by_channel(
    conn: &DatabaseConnection,
    channel_id: i32,
) -> Result<Vec<chat_channel_target::Model>, DbError> {
    Ok(chat_channel_target::Entity::find()
        .filter(chat_channel_target::Column::ChannelId.eq(channel_id))
        .order_by_desc(chat_channel_target::Column::IsDefault)
        .order_by_desc(chat_channel_target::Column::LastSeenAt)
        .all(conn)
        .await?)
}

pub async fn find_by_target(
    conn: &DatabaseConnection,
    target: &ChannelMessageTarget,
) -> Result<Option<chat_channel_target::Model>, DbError> {
    let fingerprint = fingerprint(target)?;
    Ok(chat_channel_target::Entity::find()
        .filter(chat_channel_target::Column::ChannelId.eq(target.channel_id))
        .filter(chat_channel_target::Column::Fingerprint.eq(fingerprint))
        .one(conn)
        .await?)
}

pub async fn resolve(
    conn: &DatabaseConnection,
    channel_id: i32,
    target_id: &str,
) -> Result<(chat_channel_target::Model, ChannelMessageTarget), DbError> {
    let model = find_by_target_id(conn, channel_id, target_id)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("channel target {target_id}")))?;
    if !model.is_default && model.source != SOURCE_INBOUND {
        return Err(DbError::Validation(
            "channel target is not sendable".to_string(),
        ));
    }
    let payload = crate::keyring_store::get_channel_target(target_id)
        .ok_or_else(|| DbError::NotFound(format!("channel target payload {target_id}")))?;
    let target: ChannelMessageTarget = serde_json::from_str(&payload)
        .map_err(|error| DbError::Validation(format!("invalid stored channel target: {error}")))?;
    if target.channel_id != channel_id {
        return Err(DbError::Validation("channel target mismatch".to_string()));
    }
    Ok((model, target))
}

pub async fn find_by_target_id(
    conn: &DatabaseConnection,
    channel_id: i32,
    target_id: &str,
) -> Result<Option<chat_channel_target::Model>, DbError> {
    Ok(chat_channel_target::Entity::find()
        .filter(chat_channel_target::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_target::Column::TargetId.eq(target_id))
        .one(conn)
        .await?)
}

pub async fn find_by_public_target_id(
    conn: &DatabaseConnection,
    target_id: &str,
) -> Result<Option<chat_channel_target::Model>, DbError> {
    Ok(chat_channel_target::Entity::find()
        .filter(chat_channel_target::Column::TargetId.eq(target_id))
        .one(conn)
        .await?)
}

pub async fn touch(
    conn: &DatabaseConnection,
    model: chat_channel_target::Model,
) -> Result<(), DbError> {
    let mut active = model.into_active_model();
    active.last_seen_at = Set(Utc::now());
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(())
}

pub async fn clear_default_target(
    conn: &DatabaseConnection,
    channel_id: i32,
) -> Result<(), DbError> {
    clear_other_defaults(conn, channel_id, None).await
}

async fn clear_other_defaults(
    conn: &DatabaseConnection,
    channel_id: i32,
    keep_id: Option<i32>,
) -> Result<(), DbError> {
    for model in default_targets(conn, channel_id).await? {
        if keep_id == Some(model.id) {
            continue;
        }
        if model.source == SOURCE_DEFAULT {
            chat_channel_target::Entity::delete_by_id(model.id)
                .exec(conn)
                .await?;
            if let Err(error) = crate::keyring_store::delete_channel_target(&model.target_id) {
                tracing::warn!(
                    channel_id,
                    target_id = model.target_id,
                    error = %error,
                    "orphaned secure channel target payload could not be removed"
                );
            }
            continue;
        }
        let mut active = model.into_active_model();
        active.is_default = Set(false);
        active.updated_at = Set(Utc::now());
        active.update(conn).await?;
    }
    Ok(())
}

async fn default_targets(
    conn: &DatabaseConnection,
    channel_id: i32,
) -> Result<Vec<chat_channel_target::Model>, DbError> {
    Ok(chat_channel_target::Entity::find()
        .filter(chat_channel_target::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_target::Column::IsDefault.eq(true))
        .all(conn)
        .await?)
}

struct TargetUpdate<'a> {
    payload: &'a str,
    display_name: &'a str,
    target_kind: &'a str,
    source: &'a str,
    is_default: bool,
}

async fn update_existing(
    conn: &DatabaseConnection,
    existing: chat_channel_target::Model,
    update: TargetUpdate<'_>,
) -> Result<chat_channel_target::Model, DbError> {
    crate::keyring_store::set_channel_target(&existing.target_id, update.payload)
        .map_err(DbError::Migration)?;
    let keep_default = existing.is_default || update.is_default;
    let source = if existing.source == SOURCE_INBOUND || update.source == SOURCE_INBOUND {
        SOURCE_INBOUND
    } else {
        update.source
    };
    let mut active = existing.into_active_model();
    active.display_name = Set(safe_label(update.display_name));
    active.target_kind = Set(update.target_kind.to_string());
    active.source = Set(source.to_string());
    active.is_default = Set(keep_default);
    active.last_seen_at = Set(Utc::now());
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

fn safe_label(value: &str) -> String {
    let trimmed = value.trim();
    let value = if trimmed.is_empty() {
        "消息会话"
    } else {
        trimmed
    };
    value.chars().take(128).collect()
}
