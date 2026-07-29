use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set, TransactionTrait,
};

use crate::acp::error::AcpError;
use crate::acp::version_center::inventory::{
    database_error, ManagedToolInstallation, ManagedToolSetting, ReadyToolInstallation,
    ORIGIN_MANAGED, STATUS_ACTIVE, STATUS_READY,
};
use crate::db::entities::{managed_tool_installation, managed_tool_setting};

pub async fn list_tool_installations(
    conn: &DatabaseConnection,
    tool_id: &str,
) -> Result<Vec<ManagedToolInstallation>, AcpError> {
    managed_tool_installation::Entity::find()
        .filter(managed_tool_installation::Column::ToolId.eq(tool_id))
        .all(conn)
        .await
        .map_err(database_error)
}

pub async fn list_tool_settings(
    conn: &DatabaseConnection,
) -> Result<Vec<ManagedToolSetting>, AcpError> {
    managed_tool_setting::Entity::find()
        .all(conn)
        .await
        .map_err(database_error)
}

pub async fn record_tool_ready(
    conn: &DatabaseConnection,
    input: ReadyToolInstallation<'_>,
) -> Result<(), AcpError> {
    ensure_setting(conn, input.tool_id).await?;
    let existing = managed_tool_installation::Entity::find()
        .filter(managed_tool_installation::Column::ToolId.eq(input.tool_id))
        .filter(managed_tool_installation::Column::Version.eq(input.version))
        .filter(managed_tool_installation::Column::Origin.eq(input.origin))
        .one(conn)
        .await
        .map_err(database_error)?;
    let now = Utc::now();
    match existing {
        Some(model) => update_ready(model, input, now, conn).await,
        None => insert_ready(input, now, conn).await,
    }
}

pub async fn activate_tool(
    conn: &DatabaseConnection,
    tool_id: &str,
    version: &str,
    policy: &str,
    revision: u64,
) -> Result<(), AcpError> {
    ensure_setting(conn, tool_id).await?;
    let transaction = conn.begin().await.map_err(database_error)?;
    mark_inactive(&transaction, tool_id).await?;
    mark_active(&transaction, tool_id, version).await?;
    update_pointer(&transaction, tool_id, version, policy, revision).await?;
    transaction.commit().await.map_err(database_error)
}

pub async fn set_tool_pin(
    conn: &DatabaseConnection,
    tool_id: &str,
    version: Option<String>,
) -> Result<(), AcpError> {
    ensure_setting(conn, tool_id).await?;
    let model = managed_tool_setting::Entity::find()
        .filter(managed_tool_setting::Column::ToolId.eq(tool_id))
        .one(conn)
        .await
        .map_err(database_error)?
        .ok_or_else(|| AcpError::protocol("managed tool setting is unavailable"))?;
    let mut active = model.into_active_model();
    active.pinned_version = Set(version);
    active.updated_at = Set(Utc::now());
    active.update(conn).await.map_err(database_error)?;
    Ok(())
}

async fn update_ready(
    model: ManagedToolInstallation,
    input: ReadyToolInstallation<'_>,
    now: chrono::DateTime<Utc>,
    conn: &DatabaseConnection,
) -> Result<(), AcpError> {
    let mut active = model.into_active_model();
    active.status = Set(STATUS_READY.to_string());
    active.artifact_id = Set(input.artifact_id.map(ToString::to_string));
    active.expected_sha256 = Set(input.expected_sha256.map(ToString::to_string));
    active.verified = Set(input.origin == ORIGIN_MANAGED);
    active.failure_code = Set(None);
    active.verified_at = Set((input.origin == ORIGIN_MANAGED).then_some(now));
    active.updated_at = Set(now);
    active.update(conn).await.map_err(database_error)?;
    Ok(())
}

async fn insert_ready(
    input: ReadyToolInstallation<'_>,
    now: chrono::DateTime<Utc>,
    conn: &DatabaseConnection,
) -> Result<(), AcpError> {
    managed_tool_installation::ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        tool_id: Set(input.tool_id.to_string()),
        version: Set(input.version.to_string()),
        runtime: Set(input.runtime.to_string()),
        target: Set(input.target.to_string()),
        arch: Set(input.arch.to_string()),
        origin: Set(input.origin.to_string()),
        status: Set(STATUS_READY.to_string()),
        artifact_id: Set(input.artifact_id.map(ToString::to_string)),
        expected_sha256: Set(input.expected_sha256.map(ToString::to_string)),
        verified: Set(input.origin == ORIGIN_MANAGED),
        failure_code: Set(None),
        installed_at: Set(Some(now)),
        verified_at: Set((input.origin == ORIGIN_MANAGED).then_some(now)),
        activated_at: Set(None),
        last_successful_use_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn ensure_setting(conn: &DatabaseConnection, tool_id: &str) -> Result<(), AcpError> {
    let existing = managed_tool_setting::Entity::find()
        .filter(managed_tool_setting::Column::ToolId.eq(tool_id))
        .one(conn)
        .await
        .map_err(database_error)?;
    if existing.is_some() {
        return Ok(());
    }
    let now = Utc::now();
    managed_tool_setting::ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        tool_id: Set(tool_id.to_string()),
        update_channel: Set("stable".to_string()),
        pinned_version: Set(None),
        active_version: Set(None),
        last_known_good_version: Set(None),
        update_policy: Set("recommended".to_string()),
        catalog_revision: Set(0),
        activation_generation: Set(0),
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
    tool_id: &str,
) -> Result<(), AcpError> {
    managed_tool_installation::Entity::update_many()
        .col_expr(
            managed_tool_installation::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_READY),
        )
        .filter(managed_tool_installation::Column::ToolId.eq(tool_id))
        .filter(managed_tool_installation::Column::Status.eq(STATUS_ACTIVE))
        .exec(conn)
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn mark_active<C: sea_orm::ConnectionTrait>(
    conn: &C,
    tool_id: &str,
    version: &str,
) -> Result<(), AcpError> {
    let result = managed_tool_installation::Entity::update_many()
        .col_expr(
            managed_tool_installation::Column::Status,
            sea_orm::sea_query::Expr::value(STATUS_ACTIVE),
        )
        .col_expr(
            managed_tool_installation::Column::ActivatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(managed_tool_installation::Column::ToolId.eq(tool_id))
        .filter(managed_tool_installation::Column::Version.eq(version))
        .filter(managed_tool_installation::Column::Origin.eq(ORIGIN_MANAGED))
        .filter(managed_tool_installation::Column::Verified.eq(true))
        .exec(conn)
        .await
        .map_err(database_error)?;
    (result.rows_affected == 1)
        .then_some(())
        .ok_or_else(|| AcpError::protocol("managed tool version is not ready"))
}

async fn update_pointer<C: sea_orm::ConnectionTrait>(
    conn: &C,
    tool_id: &str,
    version: &str,
    policy: &str,
    revision: u64,
) -> Result<(), AcpError> {
    let model = managed_tool_setting::Entity::find()
        .filter(managed_tool_setting::Column::ToolId.eq(tool_id))
        .one(conn)
        .await
        .map_err(database_error)?
        .ok_or_else(|| AcpError::protocol("managed tool setting is unavailable"))?;
    let next_generation = model.activation_generation + 1;
    let mut active = model.into_active_model();
    active.active_version = Set(Some(version.to_string()));
    active.update_policy = Set(policy.to_string());
    active.catalog_revision = Set(revision as i64);
    active.activation_generation = Set(next_generation);
    active.updated_at = Set(Utc::now());
    active.update(conn).await.map_err(database_error)?;
    Ok(())
}
