use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::registry::{self, AgentDistribution};
use crate::acp::version_center::{
    activate_agent, current_arch, current_target, list_agent_installations, recover_agent,
    AgentPlatformClient, ResolveAgentRequest, RUNTIME,
};
use crate::app_error::AppCommandError;
use crate::db::{service::agent_setting_service, AppDatabase};
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVersionOperationResult {
    pub agent_type: AgentType,
    pub version: String,
    pub catalog_revision: u64,
    pub activation_state: String,
}

pub async fn install_agent_version_core(
    db: &AppDatabase,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    agent_type: AgentType,
    version: String,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    let _activation_guard = connection_manager.begin_agent_activation(agent_type).await;
    let version = normalized_version(version)?;
    connection_manager
        .authorize_agent_install(agent_type)
        .await
        .map_err(acp_error)?;
    let task_id = uuid::Uuid::new_v4().to_string();
    let deferred = connection_manager.has_live_agent_session(agent_type).await;
    let resolved_version = match registry::get_agent_meta(agent_type).distribution {
        AgentDistribution::Binary { .. } => {
            crate::commands::acp::acp_download_agent_binary_core(
                agent_type,
                Some(version.clone()),
                task_id,
                db,
                emitter,
                deferred,
                "manual",
            )
            .await
            .map_err(acp_error)?;
            version.clone()
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
                deferred,
            )
            .await
            .map_err(acp_error)?
        }
    };
    operation_result(&db.conn, agent_type, &resolved_version, deferred).await
}

pub async fn switch_agent_version_core(
    conn: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    agent_type: AgentType,
    version: String,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    let _activation_guard = connection_manager.begin_agent_activation(agent_type).await;
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    let version = normalized_version(version)?;
    connection_manager
        .authorize_agent_install(agent_type)
        .await
        .map_err(acp_error)?;
    validate_local_runtime(conn, agent_type, &version).await?;
    let channel = update_channel(conn, agent_type).await?;
    let (policy, revision) = activation_metadata(conn, agent_type, &version, &channel).await?;
    let deferred = connection_manager.has_live_agent_session(agent_type).await;
    apply_activation(
        conn, agent_type, &version, &policy, revision, deferred, false,
    )
    .await?;
    crate::commands::acp::emit_acp_agents_updated(
        emitter,
        "agent_version_switched",
        Some(agent_type),
    );
    Ok(result(agent_type, version, revision, deferred))
}

pub async fn rollback_agent_version_core(
    conn: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    agent_type: AgentType,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    let _activation_guard = connection_manager.begin_agent_activation(agent_type).await;
    let _storage_work_guard = crate::acp::agent_storage_work::begin_agent_storage_work().await;
    connection_manager
        .authorize_agent_install(agent_type)
        .await
        .map_err(acp_error)?;
    let setting = setting(conn, agent_type).await?;
    let version = setting
        .last_known_good_version
        .filter(|value| setting.installed_version.as_deref() != Some(value.as_str()))
        .ok_or_else(|| AppCommandError::invalid_input("No rollback version is available"))?;
    validate_local_runtime(conn, agent_type, &version).await?;
    let (policy, revision) =
        activation_metadata(conn, agent_type, &version, &setting.update_channel).await?;
    let deferred = connection_manager.has_live_agent_session(agent_type).await;
    apply_activation(
        conn, agent_type, &version, &policy, revision, deferred, true,
    )
    .await?;
    crate::commands::acp::emit_acp_agents_updated(
        emitter,
        "agent_version_rolled_back",
        Some(agent_type),
    );
    Ok(result(agent_type, version, revision, deferred))
}

async fn operation_result(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    deferred: bool,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    validate_local_runtime(conn, agent_type, version).await?;
    let revision = crate::acp::version_center::persisted_activation_revision(
        conn, agent_type, version, deferred,
    )
    .await?;
    Ok(result(agent_type, version.to_string(), revision, deferred))
}

async fn activation_metadata(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    channel: &str,
) -> Result<(String, u64), AppCommandError> {
    match resolve(conn, agent_type, version, channel, "manual").await {
        Ok(offer) => Ok((offer.effective_update_policy, offer.revision)),
        Err(error) if fallback_allowed(&error) => Ok(("manual".to_string(), 0)),
        Err(error) => Err(error),
    }
}

async fn apply_activation(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    policy: &str,
    revision: u64,
    deferred: bool,
    recovery: bool,
) -> Result<(), AppCommandError> {
    if deferred {
        return queue_activation(agent_type, version, policy, revision).await;
    }
    let result = if recovery {
        recover_agent(conn, agent_type, version, policy, revision).await
    } else {
        activate_agent(conn, agent_type, version, policy, revision).await
    };
    result.map_err(acp_error)
}

async fn queue_activation(
    agent_type: AgentType,
    version: &str,
    policy: &str,
    revision: u64,
) -> Result<(), AppCommandError> {
    let data_dir = crate::system_skills::data_dir_from_env();
    crate::acp::version_center::push_pending_activation(
        &data_dir,
        crate::acp::version_center::PendingActivation {
            component_id: serde_json::to_string(&agent_type)
                .map_err(|error| AppCommandError::invalid_input(error.to_string()))?,
            component_kind: "agent".to_string(),
            version: version.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            policy: Some(policy.to_string()),
            revision: Some(revision),
        },
    )
    .await
}

async fn validate_local_runtime(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AppCommandError> {
    crate::acp::deepseek_config::validate_tool_version(agent_type, version)
        .map_err(AppCommandError::invalid_input)?;
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
            target: current_target(),
            arch: current_arch(),
            channel,
            reason,
        },
    )
    .await
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

fn result(
    agent_type: AgentType,
    version: String,
    revision: u64,
    deferred: bool,
) -> AgentVersionOperationResult {
    AgentVersionOperationResult {
        agent_type,
        version,
        catalog_revision: revision,
        activation_state: if deferred { "pending" } else { "active" }.to_string(),
    }
}
fn acp_error(error: AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
fn fallback_allowed(error: &AppCommandError) -> bool {
    crate::acp::version_center::fallback::allowed(error, true)
}
