use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, Set,
};

use super::conversation_session_segment_service::{
    MODE_CONTINUATION, MODE_ROOT, STATUS_ACTIVE, STATUS_CLOSED, STATUS_PENDING,
};
use crate::db::entities::{conversation, conversation_session_segment as segment};
use crate::db::error::DbError;

pub async fn persist<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    expected_external_id: Option<&str>,
    external_id: &str,
) -> Result<bool, DbError> {
    if !update_pointer(conn, conversation_id, expected_external_id, external_id).await? {
        return Ok(false);
    }
    let active = segment::Entity::find()
        .filter(segment::Column::ConversationId.eq(conversation_id))
        .filter(segment::Column::Status.eq(STATUS_ACTIVE))
        .one(conn)
        .await?;
    match active {
        None => insert_root(conn, conversation_id, external_id).await?,
        Some(active) if active.external_id.as_deref() == Some(external_id) => {}
        Some(active) => switch_active(conn, active, expected_external_id, external_id).await?,
    }
    Ok(true)
}

pub async fn replace_alias<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    external_id: &str,
) -> Result<(), DbError> {
    conversation::Entity::update_many()
        .col_expr(
            conversation::Column::ExternalId,
            sea_orm::sea_query::Expr::value(external_id.to_owned()),
        )
        .col_expr(
            conversation::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .exec(conn)
        .await?;
    let result = segment::Entity::update_many()
        .col_expr(
            segment::Column::ExternalId,
            sea_orm::sea_query::Expr::value(Some(external_id.to_owned())),
        )
        .filter(segment::Column::ConversationId.eq(conversation_id))
        .filter(segment::Column::Status.eq(STATUS_ACTIVE))
        .exec(conn)
        .await?;
    if result.rows_affected == 0 {
        insert_root(conn, conversation_id, external_id).await?;
    }
    Ok(())
}

async fn update_pointer<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    expected_external_id: Option<&str>,
    external_id: &str,
) -> Result<bool, DbError> {
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
            sea_orm::sea_query::Expr::value(external_id.to_owned()),
        )
        .col_expr(
            conversation::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(expected)
        .exec(conn)
        .await?;
    Ok(result.rows_affected == 1)
}

async fn insert_root<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    external_id: &str,
) -> Result<(), DbError> {
    let row = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("conversation {conversation_id}")))?;
    segment::ActiveModel {
        id: NotSet,
        conversation_id: Set(conversation_id),
        agent_type: Set(row.agent_type),
        external_id: Set(Some(external_id.into())),
        ordinal: Set(1),
        predecessor_id: Set(None),
        history_mode: Set(MODE_ROOT.into()),
        status: Set(STATUS_ACTIVE.into()),
        boundary_generation: Set(row.last_completed_turn_generation),
        recovery_attempt_id: Set(format!("root:{conversation_id}:{external_id}")),
        context_digest: Set(None),
        created_at: Set(Utc::now()),
        closed_at: Set(None),
    }
    .insert(conn)
    .await?;
    Ok(())
}

async fn switch_active<C: ConnectionTrait>(
    conn: &C,
    active: segment::Model,
    expected_external_id: Option<&str>,
    external_id: &str,
) -> Result<(), DbError> {
    if expected_external_id != active.external_id.as_deref() {
        return Err(DbError::Validation(
            "active conversation session changed during persistence".into(),
        ));
    }
    let pending = segment::Entity::find()
        .filter(segment::Column::ConversationId.eq(active.conversation_id))
        .filter(segment::Column::Status.eq(STATUS_PENDING))
        .order_by_desc(segment::Column::Ordinal)
        .one(conn)
        .await?;
    close_active(conn, active.id).await?;
    match pending {
        Some(pending) => activate_pending(conn, pending.id, external_id).await,
        None => insert_continuation(conn, &active, external_id).await,
    }
}

async fn close_active<C: ConnectionTrait>(conn: &C, id: i32) -> Result<(), DbError> {
    let result = segment::Entity::update_many()
        .col_expr(
            segment::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_CLOSED),
        )
        .col_expr(
            segment::Column::ClosedAt,
            sea_orm::sea_query::Expr::value(Some(Utc::now())),
        )
        .filter(segment::Column::Id.eq(id))
        .filter(segment::Column::Status.eq(STATUS_ACTIVE))
        .exec(conn)
        .await?;
    require_one(result.rows_affected, "active conversation segment")
}

async fn activate_pending<C: ConnectionTrait>(
    conn: &C,
    id: i32,
    external_id: &str,
) -> Result<(), DbError> {
    let result = segment::Entity::update_many()
        .col_expr(
            segment::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_ACTIVE),
        )
        .col_expr(
            segment::Column::ExternalId,
            sea_orm::sea_query::Expr::value(Some(external_id.to_owned())),
        )
        .filter(segment::Column::Id.eq(id))
        .filter(segment::Column::Status.eq(STATUS_PENDING))
        .exec(conn)
        .await?;
    require_one(result.rows_affected, "pending conversation segment")
}

async fn insert_continuation<C: ConnectionTrait>(
    conn: &C,
    active: &segment::Model,
    external_id: &str,
) -> Result<(), DbError> {
    segment::ActiveModel {
        id: NotSet,
        conversation_id: Set(active.conversation_id),
        agent_type: Set(active.agent_type.clone()),
        external_id: Set(Some(external_id.into())),
        ordinal: Set(active.ordinal + 1),
        predecessor_id: Set(Some(active.id)),
        history_mode: Set(MODE_CONTINUATION.into()),
        status: Set(STATUS_ACTIVE.into()),
        boundary_generation: Set(active.boundary_generation),
        recovery_attempt_id: Set(format!(
            "transition:{}:{external_id}",
            active.conversation_id
        )),
        context_digest: Set(None),
        created_at: Set(Utc::now()),
        closed_at: Set(None),
    }
    .insert(conn)
    .await?;
    Ok(())
}

fn require_one(rows: u64, target: &str) -> Result<(), DbError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(DbError::Validation(format!("stale {target}")))
    }
}
