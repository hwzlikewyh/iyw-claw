use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::registry::{self, AgentDistribution};
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::types::ResolveAgentRequest;
use crate::acp::version_center::{
    activate_agent, list_agent_installations, recover_agent, AgentPlatformClient,
};
use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::db::AppDatabase;
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVersionOperationResult {
    pub agent_type: AgentType,
    pub version: String,
    pub catalog_revision: u64,
}

pub async fn install_agent_version_core(
    db: &AppDatabase,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    agent_type: AgentType,
    version: String,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    ensure_idle(connection_manager, agent_type).await?;
    let version = normalized_version(version)?;
    let channel = update_channel(&db.conn, agent_type).await?;
    let task_id = uuid::Uuid::new_v4().to_string();
    match registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { .. } => {
            crate::commands::acp::acp_download_agent_binary_core(
                agent_type,
                Some(version.clone()),
                task_id,
                db,
                emitter,
            )
            .await
            .map_err(acp_error)?;
        }
        AgentDistribution::Npx { .. } | AgentDistribution::Uvx { .. } => {
            crate::commands::acp::acp_prepare_npx_agent_core(
                agent_type,
                Some(version.clone()),
                Some(version.clone()),
                false,
                task_id,
                db,
                emitter,
            )
            .await
            .map_err(acp_error)?;
        }
    }
    operation_result(&db.conn, agent_type, &version, &channel, "manual").await
}

pub async fn switch_agent_version_core(
    conn: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    agent_type: AgentType,
    version: String,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    ensure_idle(connection_manager, agent_type).await?;
    let version = normalized_version(version)?;
    validate_local_runtime(conn, agent_type, &version).await?;
    let channel = update_channel(conn, agent_type).await?;
    let offer = resolve(conn, agent_type, &version, &channel, "manual").await?;
    activate_agent(
        conn,
        agent_type,
        &version,
        &offer.effective_update_policy,
        offer.revision,
    )
    .await
    .map_err(acp_error)?;
    crate::commands::acp::emit_acp_agents_updated(
        emitter,
        "agent_version_switched",
        Some(agent_type),
    );
    Ok(result(agent_type, version, offer.revision))
}

pub async fn rollback_agent_version_core(
    conn: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    agent_type: AgentType,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    ensure_idle(connection_manager, agent_type).await?;
    let setting = setting(conn, agent_type).await?;
    let version = setting
        .last_known_good_version
        .filter(|value| setting.installed_version.as_deref() != Some(value.as_str()))
        .ok_or_else(|| AppCommandError::invalid_input("No rollback version is available"))?;
    validate_local_runtime(conn, agent_type, &version).await?;
    let offer = resolve(
        conn,
        agent_type,
        &version,
        &setting.update_channel,
        "manual",
    )
    .await?;
    recover_agent(
        conn,
        agent_type,
        &version,
        &offer.effective_update_policy,
        offer.revision,
    )
    .await
    .map_err(acp_error)?;
    crate::commands::acp::emit_acp_agents_updated(
        emitter,
        "agent_version_rolled_back",
        Some(agent_type),
    );
    Ok(result(agent_type, version, offer.revision))
}

async fn operation_result(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    channel: &str,
    reason: &str,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    validate_local_runtime(conn, agent_type, version).await?;
    let offer = resolve(conn, agent_type, version, channel, reason).await?;
    Ok(result(agent_type, version.to_string(), offer.revision))
}

async fn validate_local_runtime(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AppCommandError> {
    let installation = list_agent_installations(conn, agent_type)
        .await
        .map_err(acp_error)?
        .into_iter()
        .find(|item| item.version == version && item.verified)
        .ok_or_else(|| AppCommandError::invalid_input("Agent version is not installed"))?;
    let paths = AgentStoragePaths::active()
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent storage is unavailable"))?;
    let ready = match registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { cmd, .. } => {
            crate::acp::binary_cache::find_cached_binary_for_agent(&paths, agent_type, version, cmd)
                .map_err(acp_error)?
                .is_some()
        }
        AgentDistribution::Npx { cmd, .. } => {
            crate::acp::npm_runtime::resolve_private_npm_command(&paths, agent_type, version, cmd)
                .is_some()
        }
        AgentDistribution::Uvx { .. } => {
            crate::acp::binary_cache::is_uvx_agent_version_prepared(&paths, agent_type, version)
                && crate::acp::binary_cache::find_cached_uv_tool(&paths, "uvx").is_some()
        }
    };
    (ready && installation.platform == registry::current_platform())
        .then_some(())
        .ok_or_else(|| AppCommandError::invalid_input("Agent runtime is missing"))
}

async fn resolve(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    channel: &str,
    reason: &str,
) -> Result<crate::acp::version_center::AgentOffer, AppCommandError> {
    let current = setting(conn, agent_type)
        .await?
        .installed_version
        .unwrap_or_default();
    AgentPlatformClient::resolve_agent(
        conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(agent_type),
            current_version: &current,
            requested_version: Some(version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason,
        },
    )
    .await
}

async fn ensure_idle(
    manager: &ConnectionManager,
    agent_type: AgentType,
) -> Result<(), AppCommandError> {
    (!manager.has_live_agent_session(agent_type).await)
        .then_some(())
        .ok_or_else(|| {
            AppCommandError::invalid_input("Disconnect this Agent before changing versions")
        })
}

async fn update_channel(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<String, AppCommandError> {
    Ok(setting(conn, agent_type).await?.update_channel)
}

async fn setting(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<crate::db::entities::agent_setting::Model, AppCommandError> {
    agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent setting is unavailable"))
}

fn normalized_version(value: String) -> Result<String, AppCommandError> {
    let value = value.trim();
    semver::Version::parse(value)
        .map(|_| value.to_string())
        .map_err(|_| AppCommandError::invalid_input("Invalid Agent version"))
}

fn result(agent_type: AgentType, version: String, revision: u64) -> AgentVersionOperationResult {
    AgentVersionOperationResult {
        agent_type,
        version,
        catalog_revision: revision,
    }
}

fn acp_error(error: AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
