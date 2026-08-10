use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::acp::{AgentInputStatus, AgentInputStrategy};
use crate::db::entities::agent_input_outbox;
use crate::db::error::DbError;

pub async fn mark_dispatching(
    conn: &DatabaseConnection,
    id: &str,
    connection_id: &str,
    turn_generation: i64,
    strategy: AgentInputStrategy,
) -> Result<bool, DbError> {
    let statuses = [
        AgentInputStatus::Waiting.as_str(),
        AgentInputStatus::FallbackQueued.as_str(),
    ];
    let result = agent_input_outbox::Entity::update_many()
        .col_expr(
            agent_input_outbox::Column::Status,
            Expr::value(AgentInputStatus::Dispatching.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::Strategy,
            Expr::value(strategy.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::ConnectionId,
            Expr::value(connection_id),
        )
        .col_expr(
            agent_input_outbox::Column::TargetTurnGeneration,
            Expr::value(turn_generation),
        )
        .col_expr(
            agent_input_outbox::Column::DispatchAttempt,
            Expr::col(agent_input_outbox::Column::DispatchAttempt).add(1),
        )
        .col_expr(
            agent_input_outbox::Column::DispatchedAt,
            Expr::value(Utc::now()),
        )
        .filter(agent_input_outbox::Column::Id.eq(id))
        .filter(agent_input_outbox::Column::Status.is_in(statuses))
        .exec(conn)
        .await?;
    Ok(result.rows_affected == 1)
}

pub async fn transition_status(
    conn: &DatabaseConnection,
    id: &str,
    from: AgentInputStatus,
    to: AgentInputStatus,
    error: Option<String>,
) -> Result<bool, DbError> {
    let mut update = agent_input_outbox::Entity::update_many()
        .col_expr(agent_input_outbox::Column::Status, Expr::value(to.as_str()))
        .col_expr(agent_input_outbox::Column::LastError, Expr::value(error))
        .filter(agent_input_outbox::Column::Id.eq(id))
        .filter(agent_input_outbox::Column::Status.eq(from.as_str()));
    if to == AgentInputStatus::Consumed {
        update = update.col_expr(
            agent_input_outbox::Column::ConsumedAt,
            Expr::value(Utc::now()),
        );
    }
    if to == AgentInputStatus::Deleted {
        update = update.col_expr(
            agent_input_outbox::Column::DeletedAt,
            Expr::value(Utc::now()),
        );
    }
    let result = update.exec(conn).await?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_waiting(conn: &DatabaseConnection, id: &str) -> Result<bool, DbError> {
    let result = agent_input_outbox::Entity::update_many()
        .col_expr(
            agent_input_outbox::Column::Status,
            Expr::value(AgentInputStatus::Deleted.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::DeletedAt,
            Expr::value(Utc::now()),
        )
        .filter(agent_input_outbox::Column::Id.eq(id))
        .filter(agent_input_outbox::Column::Status.eq(AgentInputStatus::Waiting.as_str()))
        .exec(conn)
        .await?;
    Ok(result.rows_affected == 1)
}

pub async fn retry_failed(conn: &DatabaseConnection, id: &str) -> Result<bool, DbError> {
    let result = agent_input_outbox::Entity::update_many()
        .col_expr(
            agent_input_outbox::Column::Status,
            Expr::value(AgentInputStatus::Waiting.as_str()),
        )
        .col_expr(
            agent_input_outbox::Column::Strategy,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            agent_input_outbox::Column::TargetTurnGeneration,
            Expr::value(Option::<i64>::None),
        )
        .col_expr(
            agent_input_outbox::Column::LastError,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            agent_input_outbox::Column::DispatchedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(agent_input_outbox::Column::Id.eq(id))
        .filter(agent_input_outbox::Column::Status.eq(AgentInputStatus::Failed.as_str()))
        .exec(conn)
        .await?;
    Ok(result.rows_affected == 1)
}
