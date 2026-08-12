use std::sync::OnceLock;

use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};

use crate::acp::{AgentInputItem, AgentInputStatus};
use crate::db::entities::agent_input_outbox;
use crate::db::error::DbError;

use super::agent_input_ordering_validation::{
    load_expected_rows, movable_rows, validate_force_target, validate_locked_boundaries,
    validate_membership, validate_prefix, validate_unique,
};

const SORT_INDEX_STEP: i64 = 1024;

pub struct FreezePrefixRequest<'a> {
    pub conversation_id: i32,
    pub target_id: &'a str,
    pub expected_prefix_ids: &'a [String],
    pub batch_id: &'a str,
}

fn ordering_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub async fn lock() -> tokio::sync::MutexGuard<'static, ()> {
    ordering_lock().lock().await
}

pub async fn reorder(
    conn: &DatabaseConnection,
    conversation_id: i32,
    ordered_ids: &[String],
) -> Result<Vec<AgentInputItem>, DbError> {
    let _guard = lock().await;
    reorder_locked(conn, conversation_id, ordered_ids).await
}

async fn reorder_locked(
    conn: &DatabaseConnection,
    conversation_id: i32,
    ordered_ids: &[String],
) -> Result<Vec<AgentInputItem>, DbError> {
    validate_unique(ordered_ids)?;
    let txn = conn.begin().await?;
    let rows = ordered_rows(&txn, conversation_id).await?;
    let movable = movable_rows(&rows);
    validate_membership(&movable, ordered_ids)?;
    validate_locked_boundaries(&rows, &movable, ordered_ids)?;
    let slots = movable.iter().map(|row| row.sort_index).collect::<Vec<_>>();
    for (id, sort_index) in ordered_ids.iter().zip(slots) {
        agent_input_outbox::Entity::update_many()
            .col_expr(
                agent_input_outbox::Column::SortIndex,
                Expr::value(sort_index),
            )
            .filter(agent_input_outbox::Column::Id.eq(id))
            .exec(&txn)
            .await?;
    }
    txn.commit().await?;
    super::agent_input_outbox_service::list_visible(conn, conversation_id).await
}

pub async fn next_sort_index(
    conn: &impl sea_orm::ConnectionTrait,
    conversation_id: i32,
) -> Result<i64, DbError> {
    let latest = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .order_by_desc(agent_input_outbox::Column::SortIndex)
        .one(conn)
        .await?;
    Ok(latest
        .map(|row| row.sort_index.saturating_add(SORT_INDEX_STEP))
        .unwrap_or(0))
}

async fn ordered_rows<C>(
    conn: &C,
    conversation_id: i32,
) -> Result<Vec<agent_input_outbox::Model>, DbError>
where
    C: sea_orm::ConnectionTrait,
{
    Ok(agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .filter(agent_input_outbox::Column::ConsumedAt.is_null())
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .all(conn)
        .await?)
}

pub async fn freeze_prefix(
    conn: &DatabaseConnection,
    request: FreezePrefixRequest<'_>,
) -> Result<Vec<AgentInputItem>, DbError> {
    let _guard = lock().await;
    freeze_prefix_locked(conn, request).await
}

async fn freeze_prefix_locked(
    conn: &DatabaseConnection,
    request: FreezePrefixRequest<'_>,
) -> Result<Vec<AgentInputItem>, DbError> {
    validate_unique(request.expected_prefix_ids)?;
    let txn = conn.begin().await?;
    let rows = ordered_rows(&txn, request.conversation_id).await?;
    let target = validate_force_target(&rows, request.target_id)?;
    let expected_rows = load_expected_rows(&txn, &request).await?;
    validate_prefix(&rows, target, &expected_rows, &request)?;
    apply_force_batch(&txn, &rows[..=target], request.batch_id).await?;
    txn.commit().await?;
    super::agent_input_outbox_service::list_force_batch(conn, request.batch_id).await
}

async fn apply_force_batch(
    txn: &DatabaseTransaction,
    prefix: &[agent_input_outbox::Model],
    batch_id: &str,
) -> Result<(), DbError> {
    let requested_at = Utc::now();
    for row in prefix {
        let mut update = agent_input_outbox::Entity::update_many()
            .col_expr(
                agent_input_outbox::Column::ForceBatchId,
                Expr::value(batch_id),
            )
            .col_expr(
                agent_input_outbox::Column::ForceRequestedAt,
                Expr::value(requested_at),
            )
            .filter(agent_input_outbox::Column::Id.eq(&row.id))
            .filter(agent_input_outbox::Column::ConsumedAt.is_null());
        if row.status == AgentInputStatus::Dispatching.as_str() {
            update = update
                .col_expr(
                    agent_input_outbox::Column::Status,
                    Expr::value(AgentInputStatus::FallbackQueued.as_str()),
                )
                .col_expr(
                    agent_input_outbox::Column::LastError,
                    Expr::value("cooperative feedback withdrawn for force batch"),
                );
        }
        update.exec(txn).await?;
    }
    Ok(())
}
