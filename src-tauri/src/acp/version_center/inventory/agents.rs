use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set, TransactionTrait,
};

use crate::acp::error::AcpError;
use crate::acp::version_center::inventory::{
    database_error, serialize_agent_type, AgentInstallation, ReadyAgentInstallation, STATUS_ACTIVE,
    STATUS_READY,
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

pub async fn record_agent_ready(
    conn: &DatabaseConnection,
    input: ReadyAgentInstallation<'_>,
) -> Result<(), AcpError> {
    let agent_type = serialize_agent_type(input.agent_type)?;
    let existing = agent_installation::Entity::find()
        .filter(agent_installation::Column::AgentType.eq(agent_type.clone()))
        .filter(agent_installation::Column::Version.eq(input.version))
        .filter(agent_installation::Column::Platform.eq(input.platform))
        .one(conn)
        .await
        .map_err(database_error)?;
    let now = Utc::now();
    match existing {
        Some(model) => update_ready(model, input, now, conn).await,
        None => insert_ready(agent_type, input, now, conn).await,
    }
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

async fn update_ready(
    model: AgentInstallation,
    input: ReadyAgentInstallation<'_>,
    now: chrono::DateTime<Utc>,
    conn: &DatabaseConnection,
) -> Result<(), AcpError> {
    let mut active = model.into_active_model();
    active.status = Set(STATUS_READY.to_string());
    active.delivery_kind = Set(input.delivery_kind.to_string());
    active.artifact_id = Set(input.artifact_id.map(ToString::to_string));
    active.source_key = Set(input.source_key.map(ToString::to_string));
    active.expected_sha256 = Set(input.expected_sha256.map(ToString::to_string));
    active.verified = Set(true);
    active.failure_code = Set(None);
    active.verified_at = Set(Some(now));
    active.updated_at = Set(now);
    active.update(conn).await.map_err(database_error)?;
    Ok(())
}

async fn insert_ready(
    agent_type: String,
    input: ReadyAgentInstallation<'_>,
    now: chrono::DateTime<Utc>,
    conn: &DatabaseConnection,
) -> Result<(), AcpError> {
    agent_installation::ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        agent_type: Set(agent_type),
        registry_id: Set(input.registry_id.to_string()),
        version: Set(input.version.to_string()),
        platform: Set(input.platform.to_string()),
        status: Set(STATUS_READY.to_string()),
        delivery_kind: Set(input.delivery_kind.to_string()),
        artifact_id: Set(input.artifact_id.map(ToString::to_string)),
        source_key: Set(input.source_key.map(ToString::to_string)),
        expected_sha256: Set(input.expected_sha256.map(ToString::to_string)),
        verified: Set(true),
        failure_code: Set(None),
        installed_at: Set(Some(now)),
        verified_at: Set(Some(now)),
        activated_at: Set(None),
        last_successful_launch_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await
    .map_err(database_error)?;
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
