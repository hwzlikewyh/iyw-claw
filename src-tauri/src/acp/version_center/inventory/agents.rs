use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set, TransactionTrait,
};

use crate::acp::error::AcpError;
use crate::acp::version_center::inventory::{
    database_error, serialize_agent_type, AgentInstallation, STATUS_ACTIVE, STATUS_READY,
};
use crate::db::entities::{agent_installation, agent_setting};
use crate::models::agent::AgentType;

pub async fn list_agent_installations(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<Vec<AgentInstallation>, AcpError> {
    let agent_type = serialize_agent_type(agent_type)?;
    agent_installation::Entity::find()
        .filter(agent_installation::Column::AgentType.eq(agent_type))
        .all(conn)
        .await
        .map_err(database_error)
}

pub async fn activate_agent(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    policy: &str,
    revision: u64,
) -> Result<(), AcpError> {
    let encoded = serialize_agent_type(agent_type)?;
    let transaction = conn.begin().await.map_err(database_error)?;
    mark_inactive(&transaction, &encoded).await?;
    mark_active(&transaction, &encoded, version).await?;
    update_pointer(&transaction, &encoded, version, policy, revision).await?;
    transaction.commit().await.map_err(database_error)
}

pub async fn set_agent_pin(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: Option<String>,
) -> Result<(), AcpError> {
    let encoded = serialize_agent_type(agent_type)?;
    let model = agent_setting::Entity::find()
        .filter(agent_setting::Column::AgentType.eq(encoded))
        .one(conn)
        .await
        .map_err(database_error)?
        .ok_or_else(|| AcpError::protocol("Agent setting is unavailable"))?;
    let mut active = model.into_active_model();
    active.pinned_version = Set(version);
    active.updated_at = Set(Utc::now());
    active.update(conn).await.map_err(database_error)?;
    Ok(())
}

async fn mark_inactive<C: sea_orm::ConnectionTrait>(
    conn: &C,
    agent_type: &str,
) -> Result<(), AcpError> {
    agent_installation::Entity::update_many()
        .col_expr(
            agent_installation::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_READY),
        )
        .filter(agent_installation::Column::AgentType.eq(agent_type))
        .filter(agent_installation::Column::Status.eq(STATUS_ACTIVE))
        .exec(conn)
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn mark_active<C: sea_orm::ConnectionTrait>(
    conn: &C,
    agent_type: &str,
    version: &str,
) -> Result<(), AcpError> {
    let result = agent_installation::Entity::update_many()
        .col_expr(
            agent_installation::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_ACTIVE),
        )
        .col_expr(
            agent_installation::Column::ActivatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(agent_installation::Column::AgentType.eq(agent_type))
        .filter(agent_installation::Column::Version.eq(version))
        .filter(agent_installation::Column::Verified.eq(true))
        .exec(conn)
        .await
        .map_err(database_error)?;
    (result.rows_affected == 1)
        .then_some(())
        .ok_or_else(|| AcpError::protocol("Agent version is not ready"))
}

async fn update_pointer<C: sea_orm::ConnectionTrait>(
    conn: &C,
    agent_type: &str,
    version: &str,
    policy: &str,
    revision: u64,
) -> Result<(), AcpError> {
    let model = agent_setting::Entity::find()
        .filter(agent_setting::Column::AgentType.eq(agent_type))
        .one(conn)
        .await
        .map_err(database_error)?
        .ok_or_else(|| AcpError::protocol("Agent setting is unavailable"))?;
    let next_generation = model.activation_generation + 1;
    let mut active = model.into_active_model();
    active.installed_version = Set(Some(version.to_string()));
    active.update_policy = Set(policy.to_string());
    active.catalog_revision = Set(revision as i64);
    active.activation_generation = Set(next_generation);
    active.updated_at = Set(Utc::now());
    active.update(conn).await.map_err(database_error)?;
    Ok(())
}
