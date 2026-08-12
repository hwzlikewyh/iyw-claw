use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};

use crate::acp::{AgentInputItem, AgentInputStatus, AgentInputStrategy};
use crate::db::entities::agent_input_outbox;
use crate::db::error::DbError;

use super::agent_input_outbox_service::parse_model;

pub struct ForceBatchClaim<'a> {
    pub batch_id: &'a str,
    pub connection_id: &'a str,
    pub turn_generation: i64,
}

pub struct ForceBatchTransition<'a> {
    pub batch_id: &'a str,
    pub turn_generation: i64,
    pub from: AgentInputStatus,
    pub to: AgentInputStatus,
    pub error: Option<String>,
}

pub async fn mark_force_batch_dispatching(
    conn: &DatabaseConnection,
    claim: ForceBatchClaim<'_>,
) -> Result<u64, DbError> {
    let statuses = [
        AgentInputStatus::Waiting.as_str(),
        AgentInputStatus::FallbackQueued.as_str(),
    ];
    let txn = conn.begin().await?;
    let members = transaction_members(&txn, claim.batch_id).await?;
    let Some(expected) = valid_claim_count(&members, &statuses) else {
        txn.rollback().await?;
        return Err(DbError::Validation(
            "force batch changed before dispatch claim".into(),
        ));
    };
    let changed = apply_dispatch_claim(&txn, &claim, statuses).await?;
    if changed != expected {
        txn.rollback().await?;
        return Err(DbError::Validation(
            "force batch was only partially claimed".into(),
        ));
    }
    txn.commit().await?;
    Ok(changed)
}

async fn apply_dispatch_claim(
    txn: &DatabaseTransaction,
    claim: &ForceBatchClaim<'_>,
    statuses: [&str; 2],
) -> Result<u64, DbError> {
    let result = agent_input_outbox::Entity::update_many()
        .col_expr(
            agent_input_outbox::Column::Status,
            Expr::value(AgentInputStatus::Dispatching.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::Strategy,
            Expr::value(AgentInputStrategy::SafeForceNext.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::ConnectionId,
            Expr::value(claim.connection_id),
        )
        .col_expr(
            agent_input_outbox::Column::TargetTurnGeneration,
            Expr::value(claim.turn_generation),
        )
        .col_expr(
            agent_input_outbox::Column::DispatchAttempt,
            Expr::col(agent_input_outbox::Column::DispatchAttempt).add(1),
        )
        .col_expr(
            agent_input_outbox::Column::DispatchedAt,
            Expr::value(Utc::now()),
        )
        .filter(agent_input_outbox::Column::ForceBatchId.eq(claim.batch_id))
        .filter(agent_input_outbox::Column::Status.is_in(statuses))
        .filter(agent_input_outbox::Column::ConsumedAt.is_null())
        .exec(txn)
        .await?;
    Ok(result.rows_affected)
}

pub async fn transition_force_batch(
    conn: &DatabaseConnection,
    transition: ForceBatchTransition<'_>,
) -> Result<u64, DbError> {
    let txn = conn.begin().await?;
    let members = transaction_members(&txn, transition.batch_id).await?;
    let Some(expected) = valid_transition_count(&members, &transition) else {
        txn.rollback().await?;
        return Err(DbError::Validation(
            "force batch changed before terminal settlement".into(),
        ));
    };
    let changed = apply_transition(&txn, transition).await?;
    if changed != expected {
        txn.rollback().await?;
        return Err(DbError::Validation(
            "force batch was only partially settled".into(),
        ));
    }
    txn.commit().await?;
    Ok(changed)
}

async fn apply_transition(
    txn: &DatabaseTransaction,
    transition: ForceBatchTransition<'_>,
) -> Result<u64, DbError> {
    let mut update = agent_input_outbox::Entity::update_many()
        .col_expr(
            agent_input_outbox::Column::Status,
            Expr::value(transition.to.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::LastError,
            Expr::value(transition.error),
        )
        .filter(agent_input_outbox::Column::ForceBatchId.eq(transition.batch_id))
        .filter(agent_input_outbox::Column::TargetTurnGeneration.eq(transition.turn_generation))
        .filter(agent_input_outbox::Column::Strategy.eq(AgentInputStrategy::SafeForceNext.as_str()))
        .filter(agent_input_outbox::Column::Status.eq(transition.from.as_str()));
    if matches!(
        transition.to,
        AgentInputStatus::Consumed | AgentInputStatus::Failed
    ) {
        update = update
            .col_expr(
                agent_input_outbox::Column::ForceBatchId,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                agent_input_outbox::Column::ForceRequestedAt,
                Expr::value(Option::<chrono::DateTime<Utc>>::None),
            );
    }
    if transition.to == AgentInputStatus::Consumed {
        update = update.col_expr(
            agent_input_outbox::Column::ConsumedAt,
            Expr::value(Utc::now()),
        );
    }
    let result = update.exec(txn).await?;
    Ok(result.rows_affected)
}

async fn transaction_members(
    txn: &DatabaseTransaction,
    batch_id: &str,
) -> Result<Vec<agent_input_outbox::Model>, DbError> {
    Ok(agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ForceBatchId.eq(batch_id))
        .filter(agent_input_outbox::Column::ConsumedAt.is_null())
        .all(txn)
        .await?)
}

fn valid_claim_count(members: &[agent_input_outbox::Model], statuses: &[&str]) -> Option<u64> {
    (!members.is_empty()
        && members
            .iter()
            .all(|item| statuses.contains(&item.status.as_str())))
    .then_some(members.len() as u64)
}

fn valid_transition_count(
    members: &[agent_input_outbox::Model],
    transition: &ForceBatchTransition<'_>,
) -> Option<u64> {
    (!members.is_empty()
        && members.iter().all(|item| {
            item.status == transition.from.as_str()
                && item.target_turn_generation == Some(transition.turn_generation)
                && item.strategy.as_deref() == Some(AgentInputStrategy::SafeForceNext.as_str())
        }))
    .then_some(members.len() as u64)
}

pub async fn fail_force_batch(
    conn: &DatabaseConnection,
    batch_id: &str,
    error: String,
) -> Result<u64, DbError> {
    let statuses = [
        AgentInputStatus::Waiting.as_str(),
        AgentInputStatus::Dispatching.as_str(),
        AgentInputStatus::FallbackQueued.as_str(),
    ];
    let result = agent_input_outbox::Entity::update_many()
        .col_expr(
            agent_input_outbox::Column::Status,
            Expr::value(AgentInputStatus::Failed.as_str()),
        )
        .col_expr(agent_input_outbox::Column::LastError, Expr::value(error))
        .col_expr(
            agent_input_outbox::Column::ForceBatchId,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            agent_input_outbox::Column::ForceRequestedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(agent_input_outbox::Column::ForceBatchId.eq(batch_id))
        .filter(agent_input_outbox::Column::Status.is_in(statuses))
        .filter(agent_input_outbox::Column::ConsumedAt.is_null())
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}

pub async fn list_force_batch(
    conn: &DatabaseConnection,
    batch_id: &str,
) -> Result<Vec<AgentInputItem>, DbError> {
    let rows = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ForceBatchId.eq(batch_id))
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .filter(agent_input_outbox::Column::ConsumedAt.is_null())
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .all(conn)
        .await?;
    rows.into_iter().map(parse_model).collect()
}

pub async fn active_force_batch(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<AgentInputItem>, DbError> {
    let row = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .filter(agent_input_outbox::Column::ForceBatchId.is_not_null())
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .filter(agent_input_outbox::Column::ConsumedAt.is_null())
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .one(conn)
        .await?;
    match row.and_then(|value| value.force_batch_id) {
        Some(batch_id) => list_force_batch(conn, &batch_id).await,
        None => Ok(Vec::new()),
    }
}
