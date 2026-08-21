use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    QueryFilter, Set, TransactionTrait,
};
use sha2::{Digest, Sha256};

use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::entities::{chat_channel_conversation_binding, conversation};
use crate::db::error::DbError;

const ROUTE_VERSION: &str = "v1";
const HMAC_BLOCK_BYTES: usize = 64;
const HMAC_INNER_PAD: u8 = 0x36;
const HMAC_OUTER_PAD: u8 = 0x5c;

#[derive(Debug, Clone)]
pub struct ConversationRoute {
    pub route_key: String,
    pub target_id: String,
}

pub struct BindingRollback<'a> {
    pub channel_id: i32,
    pub route_key: &'a str,
    pub failed_conversation_id: i32,
    pub previous: Option<(&'a str, i32)>,
}

pub async fn resolve_route(
    conn: &DatabaseConnection,
    target: &ChannelMessageTarget,
    sender_id: &str,
    metadata: &serde_json::Value,
) -> Result<ConversationRoute, DbError> {
    let target_row = super::chat_channel_target_service::find_by_target(conn, target)
        .await?
        .ok_or_else(|| DbError::NotFound("registered channel target".to_string()))?;
    Ok(ConversationRoute {
        route_key: route_key(target, sender_id, metadata)?,
        target_id: target_row.target_id,
    })
}

pub fn registered_route(
    target_id: String,
    target: &ChannelMessageTarget,
    sender_id: &str,
    metadata: &serde_json::Value,
) -> Result<ConversationRoute, DbError> {
    Ok(ConversationRoute {
        route_key: route_key(target, sender_id, metadata)?,
        target_id,
    })
}

pub fn route_key(
    target: &ChannelMessageTarget,
    sender_id: &str,
    metadata: &serde_json::Value,
) -> Result<String, DbError> {
    let material = route_material(target, sender_id, metadata)?;
    let secret =
        crate::keyring_store::get_or_create_channel_target_secret().map_err(DbError::Migration)?;
    let digest = hmac_sha256(secret.as_bytes(), material.as_bytes());
    Ok(format!("{ROUTE_VERSION}:{digest}"))
}

fn route_material(
    target: &ChannelMessageTarget,
    sender_id: &str,
    metadata: &serde_json::Value,
) -> Result<String, DbError> {
    let chat_id = target.chat_id.as_deref().unwrap_or(sender_id);
    let value = if let Some(thread_key) = non_empty(target.thread_key.as_deref()) {
        serde_json::json!({
            "scope": "thread",
            "channel_id": target.channel_id,
            "chat_id": chat_id,
            "thread_kind": target.thread_kind,
            "thread_key": thread_key,
        })
    } else if is_group_chat(target, metadata) {
        serde_json::json!({
            "scope": "group_sender",
            "channel_id": target.channel_id,
            "chat_id": chat_id,
            "sender_id": sender_id,
        })
    } else {
        serde_json::json!({
            "scope": "peer",
            "channel_id": target.channel_id,
            "peer_id": chat_id,
        })
    };
    serde_json::to_string(&value)
        .map_err(|error| DbError::Validation(format!("invalid channel route: {error}")))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn is_group_chat(target: &ChannelMessageTarget, metadata: &serde_json::Value) -> bool {
    chat_type_value(metadata)
        .or_else(|| target.provider_payload.as_ref().and_then(chat_type_value))
        .unwrap_or(false)
}

fn chat_type_value(value: &serde_json::Value) -> Option<bool> {
    const TYPE_KEYS: [&str; 5] = [
        "chat_type",
        "chattype",
        "conversation_type",
        "conversationType",
        "chatType",
    ];
    for key in TYPE_KEYS {
        if let Some(result) = raw_chat_type(value.get(key)) {
            return Some(result);
        }
    }
    for key in ["message", "event", "conversation"] {
        if let Some(result) = value.get(key).and_then(chat_type_value) {
            return Some(result);
        }
    }
    None
}

fn raw_chat_type(raw: Option<&serde_json::Value>) -> Option<bool> {
    let raw = raw?;
    if let Some(number) = raw.as_i64() {
        return match number {
            2 => Some(true),
            1 => Some(false),
            _ => None,
        };
    }
    if let Some(number) = raw.as_u64() {
        return match number {
            2 => Some(true),
            1 => Some(false),
            _ => None,
        };
    }
    match raw.as_str()?.trim().to_ascii_lowercase().as_str() {
        "2" | "group" | "group_chat" | "chat" | "room" => Some(true),
        "1" | "private" | "single" | "direct" | "dm" | "p2p" => Some(false),
        _ => None,
    }
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> String {
    let key = normalized_hmac_key(key);
    let mut inner_pad = [HMAC_INNER_PAD; HMAC_BLOCK_BYTES];
    let mut outer_pad = [HMAC_OUTER_PAD; HMAC_BLOCK_BYTES];
    for index in 0..HMAC_BLOCK_BYTES {
        inner_pad[index] ^= key[index];
        outer_pad[index] ^= key[index];
    }
    let inner = Sha256::new()
        .chain_update(inner_pad)
        .chain_update(message)
        .finalize();
    let digest = Sha256::new()
        .chain_update(outer_pad)
        .chain_update(inner)
        .finalize();
    format!("{digest:x}")
}

fn normalized_hmac_key(key: &[u8]) -> [u8; HMAC_BLOCK_BYTES] {
    let mut normalized = [0u8; HMAC_BLOCK_BYTES];
    if key.len() > HMAC_BLOCK_BYTES {
        let digest = Sha256::digest(key);
        normalized[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    normalized
}

pub async fn find_by_route<C: ConnectionTrait>(
    conn: &C,
    channel_id: i32,
    route_key: &str,
) -> Result<Option<chat_channel_conversation_binding::Model>, DbError> {
    Ok(chat_channel_conversation_binding::Entity::find()
        .filter(chat_channel_conversation_binding::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_conversation_binding::Column::RouteKey.eq(route_key))
        .one(conn)
        .await?)
}

pub async fn bind(
    conn: &DatabaseConnection,
    channel_id: i32,
    route: &ConversationRoute,
    conversation_id: i32,
) -> Result<chat_channel_conversation_binding::Model, DbError> {
    let txn = conn.begin().await?;
    upsert_route(&txn, channel_id, route, conversation_id).await?;
    let saved = find_by_route(&txn, channel_id, &route.route_key)
        .await?
        .ok_or_else(|| DbError::Migration("conversation binding upsert disappeared".into()))?;
    txn.commit().await?;
    Ok(saved)
}

/// Atomically persists an Agent session id and, for channel sessions, the
/// durable route that owns it. A stale CAS returns `false` without changing
/// either row; failures after the CAS roll the session-id update back too.
pub async fn persist_session_start(
    conn: &DatabaseConnection,
    conversation_id: i32,
    expected_external_id: Option<&str>,
    external_id: &str,
    route: Option<(i32, &ConversationRoute)>,
) -> Result<bool, DbError> {
    let txn = conn.begin().await?;
    let outcome: Result<bool, DbError> = async {
        if !update_external_id_if_matches(&txn, conversation_id, expected_external_id, external_id)
            .await?
        {
            return Ok(false);
        }
        if let Some((channel_id, route)) = route {
            upsert_route(&txn, channel_id, route, conversation_id).await?;
        }
        Ok(true)
    }
    .await;

    match outcome {
        Ok(true) => {
            txn.commit().await?;
            Ok(true)
        }
        Ok(false) => {
            txn.rollback().await?;
            Ok(false)
        }
        Err(error) => {
            if let Err(rollback_error) = txn.rollback().await {
                tracing::error!(
                    operation_error = %error,
                    rollback_error = %rollback_error,
                    "failed to roll back channel session-start persistence"
                );
                return Err(rollback_error.into());
            }
            Err(error)
        }
    }
}

async fn update_external_id_if_matches<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    expected_external_id: Option<&str>,
    external_id: &str,
) -> Result<bool, DbError> {
    use sea_orm::sea_query::Expr;

    let expected = match expected_external_id {
        Some(value) => sea_orm::Condition::any()
            .add(conversation::Column::ExternalId.eq(value))
            .add(conversation::Column::ExternalId.eq(external_id)),
        None => sea_orm::Condition::any()
            .add(conversation::Column::ExternalId.is_null())
            .add(conversation::Column::ExternalId.eq(external_id)),
    };
    let result = conversation::Entity::update_many()
        .col_expr(
            conversation::Column::ExternalId,
            Expr::value(external_id.to_string()),
        )
        .col_expr(conversation::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(expected)
        .exec(conn)
        .await?;
    Ok(result.rows_affected > 0)
}

async fn upsert_route<C: ConnectionTrait>(
    conn: &C,
    channel_id: i32,
    route: &ConversationRoute,
    conversation_id: i32,
) -> Result<(), DbError> {
    let now = Utc::now();
    chat_channel_conversation_binding::Entity::insert(
        chat_channel_conversation_binding::ActiveModel {
            id: NotSet,
            channel_id: Set(channel_id),
            route_key: Set(route.route_key.clone()),
            target_id: Set(route.target_id.clone()),
            conversation_id: Set(conversation_id),
            created_at: Set(now),
            updated_at: Set(now),
        },
    )
    .on_conflict(
        OnConflict::columns([
            chat_channel_conversation_binding::Column::ChannelId,
            chat_channel_conversation_binding::Column::RouteKey,
        ])
        .update_columns([
            chat_channel_conversation_binding::Column::TargetId,
            chat_channel_conversation_binding::Column::ConversationId,
            chat_channel_conversation_binding::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(conn)
    .await?;
    Ok(())
}

/// Roll back a candidate binding only while the route still points at that
/// candidate. A newer successful session therefore wins any cleanup race.
pub async fn rollback_if_current(
    conn: &DatabaseConnection,
    rollback: BindingRollback<'_>,
) -> Result<bool, DbError> {
    let filter = chat_channel_conversation_binding::Entity::update_many()
        .filter(chat_channel_conversation_binding::Column::ChannelId.eq(rollback.channel_id))
        .filter(chat_channel_conversation_binding::Column::RouteKey.eq(rollback.route_key))
        .filter(
            chat_channel_conversation_binding::Column::ConversationId
                .eq(rollback.failed_conversation_id),
        );
    let affected = match rollback.previous {
        Some((target_id, conversation_id)) => {
            filter
                .col_expr(
                    chat_channel_conversation_binding::Column::TargetId,
                    sea_orm::sea_query::Expr::value(target_id.to_string()),
                )
                .col_expr(
                    chat_channel_conversation_binding::Column::ConversationId,
                    sea_orm::sea_query::Expr::value(conversation_id),
                )
                .col_expr(
                    chat_channel_conversation_binding::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(Utc::now()),
                )
                .exec(conn)
                .await?
                .rows_affected
        }
        None => {
            chat_channel_conversation_binding::Entity::delete_many()
                .filter(
                    chat_channel_conversation_binding::Column::ChannelId.eq(rollback.channel_id),
                )
                .filter(chat_channel_conversation_binding::Column::RouteKey.eq(rollback.route_key))
                .filter(
                    chat_channel_conversation_binding::Column::ConversationId
                        .eq(rollback.failed_conversation_id),
                )
                .exec(conn)
                .await?
                .rows_affected
        }
    };
    Ok(affected > 0)
}
