use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};

use crate::db::entities::{conversation, conversation_session_segment as segment};
use crate::db::error::DbError;

pub const MODE_ROOT: &str = "root";
pub const MODE_CONTINUATION: &str = "continuation";
pub const STATUS_PENDING: &str = "pending";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_CLOSED: &str = "closed";
pub const STATUS_FAILED: &str = "failed";

pub async fn list_for_conversation(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<segment::Model>, DbError> {
    Ok(segment::Entity::find()
        .filter(segment::Column::ConversationId.eq(conversation_id))
        .order_by_asc(segment::Column::Ordinal)
        .all(conn)
        .await?)
}

pub async fn ensure_active<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    external_id: &str,
) -> Result<(), DbError> {
    if let Some(active) = active_segment(conn, conversation_id).await? {
        return require_external_id(&active, external_id);
    }
    insert_root_if_absent(conn, conversation_id, external_id).await?;
    let active = active_segment(conn, conversation_id)
        .await?
        .ok_or_else(|| DbError::Migration("active conversation segment was not created".into()))?;
    require_external_id(&active, external_id)
}

pub async fn reserve_continuation(
    conn: &DatabaseConnection,
    conversation_id: i32,
    expected_external_id: &str,
    recovery_attempt_id: &str,
    context_digest: Option<&str>,
) -> Result<segment::Model, DbError> {
    let txn = conn.begin().await?;
    let result = reserve_in_transaction(
        &txn,
        conversation_id,
        expected_external_id,
        recovery_attempt_id,
        context_digest,
    )
    .await;
    match result {
        Ok(model) => {
            txn.commit().await?;
            Ok(model)
        }
        Err(error) => {
            txn.rollback().await?;
            Err(error)
        }
    }
}

pub async fn fail_pending(
    conn: &DatabaseConnection,
    recovery_attempt_id: &str,
) -> Result<bool, DbError> {
    let result = segment::Entity::update_many()
        .col_expr(
            segment::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_FAILED),
        )
        .col_expr(
            segment::Column::ClosedAt,
            sea_orm::sea_query::Expr::value(Some(Utc::now())),
        )
        .filter(segment::Column::RecoveryAttemptId.eq(recovery_attempt_id))
        .filter(segment::Column::Status.eq(STATUS_PENDING))
        .exec(conn)
        .await?;
    Ok(result.rows_affected > 0)
}

async fn reserve_in_transaction<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    expected_external_id: &str,
    recovery_attempt_id: &str,
    context_digest: Option<&str>,
) -> Result<segment::Model, DbError> {
    if let Some(existing) = segment_by_attempt(conn, recovery_attempt_id).await? {
        return Ok(existing);
    }
    if let Some(existing) = pending_segment(conn, conversation_id).await? {
        return Ok(existing);
    }
    let conversation = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("conversation {conversation_id}")))?;
    if conversation.external_id.as_deref() != Some(expected_external_id) {
        return Err(DbError::Validation(
            "conversation session changed before recovery was reserved".into(),
        ));
    }
    let active = active_segment(conn, conversation_id)
        .await?
        .ok_or_else(|| DbError::NotFound("active conversation session segment".into()))?;
    require_external_id(&active, expected_external_id)?;
    let model = segment::ActiveModel {
        id: NotSet,
        conversation_id: Set(conversation_id),
        agent_type: Set(active.agent_type.clone()),
        external_id: Set(None),
        ordinal: Set(active.ordinal + 1),
        predecessor_id: Set(Some(active.id)),
        history_mode: Set(MODE_CONTINUATION.into()),
        status: Set(STATUS_PENDING.into()),
        boundary_generation: Set(conversation.last_completed_turn_generation),
        recovery_attempt_id: Set(recovery_attempt_id.into()),
        context_digest: Set(context_digest.map(str::to_owned)),
        created_at: Set(Utc::now()),
        closed_at: Set(None),
    };
    Ok(model.insert(conn).await?)
}

async fn insert_root_if_absent<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    external_id: &str,
) -> Result<(), DbError> {
    let row = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("conversation {conversation_id}")))?;
    let model = segment::ActiveModel {
        id: NotSet,
        conversation_id: Set(conversation_id),
        agent_type: Set(row.agent_type),
        external_id: Set(Some(external_id.into())),
        ordinal: Set(1),
        predecessor_id: Set(None),
        history_mode: Set(MODE_ROOT.into()),
        status: Set(STATUS_ACTIVE.into()),
        boundary_generation: Set(row.last_completed_turn_generation),
        recovery_attempt_id: Set(format!("bind:{conversation_id}:{external_id}")),
        context_digest: Set(None),
        created_at: Set(Utc::now()),
        closed_at: Set(None),
    };
    match model.insert(conn).await {
        Ok(_) => Ok(()),
        Err(_) if active_segment(conn, conversation_id).await?.is_some() => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn active_segment<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
) -> Result<Option<segment::Model>, DbError> {
    Ok(segment::Entity::find()
        .filter(segment::Column::ConversationId.eq(conversation_id))
        .filter(segment::Column::Status.eq(STATUS_ACTIVE))
        .one(conn)
        .await?)
}

async fn segment_by_attempt<C: ConnectionTrait>(
    conn: &C,
    recovery_attempt_id: &str,
) -> Result<Option<segment::Model>, DbError> {
    Ok(segment::Entity::find()
        .filter(segment::Column::RecoveryAttemptId.eq(recovery_attempt_id))
        .one(conn)
        .await?)
}

async fn pending_segment<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
) -> Result<Option<segment::Model>, DbError> {
    Ok(segment::Entity::find()
        .filter(segment::Column::ConversationId.eq(conversation_id))
        .filter(segment::Column::Status.eq(STATUS_PENDING))
        .one(conn)
        .await?)
}

fn require_external_id(model: &segment::Model, external_id: &str) -> Result<(), DbError> {
    if model.external_id.as_deref() == Some(external_id) {
        return Ok(());
    }
    Err(DbError::Validation(
        "conversation session changed during recovery".into(),
    ))
}
