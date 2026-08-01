use sea_orm::DatabaseConnection;
use serde::Serialize;

#[path = "agent_version_center_requests.rs"]
mod requests;

pub use requests::{
    AgentHistoryRequest, AgentPinRequest, ToolHistoryRequest, ToolInstallRequest, ToolPinRequest,
};

use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::registry;
use crate::acp::version_center::{
    install_managed_tool, known_tool, list_agent_installations, list_tool_installations,
    list_tool_settings, set_agent_pin, set_tool_pin, AgentPlatformClient, CatalogStore,
    CatalogView, ManagedToolInstallResult, TOOL_IDS,
};
use crate::acp::version_center::{ManagedToolInstallation, ManagedToolSetting};
use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::models::agent::AgentType;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVersionCenterSnapshot {
    pub catalog: CatalogView,
    pub agents: Vec<AgentVersionInventory>,
    pub tool_settings: Vec<ManagedToolSetting>,
    pub tool_installations: Vec<ManagedToolInstallation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVersionInventory {
    pub agent_type: AgentType,
    pub registry_id: String,
    pub active_version: Option<String>,
    pub pinned_version: Option<String>,
    pub last_known_good_version: Option<String>,
    pub update_channel: String,
    pub update_policy: String,
    pub catalog_revision: i64,
    pub activation_generation: i64,
    pub installations: Vec<crate::acp::version_center::AgentInstallation>,
}

pub async fn snapshot_core(
    conn: &DatabaseConnection,
    catalog: &CatalogStore,
) -> Result<AgentVersionCenterSnapshot, AppCommandError> {
    let settings = agent_setting_service::list(conn)
        .await
        .map_err(AppCommandError::from)?;
    let mut agents = Vec::new();
    for setting in settings {
        let Ok(agent_type) = serde_json::from_str::<AgentType>(&setting.agent_type) else {
            continue;
        };
        let installations = list_agent_installations(conn, agent_type)
            .await
            .map_err(acp_error)?;
        agents.push(AgentVersionInventory {
            agent_type,
            registry_id: setting.registry_id,
            active_version: setting.installed_version,
            pinned_version: setting.pinned_version,
            last_known_good_version: setting.last_known_good_version,
            update_channel: setting.update_channel,
            update_policy: setting.update_policy,
            catalog_revision: setting.catalog_revision,
            activation_generation: setting.activation_generation,
            installations,
        });
    }
    let mut tool_installations = Vec::new();
    for tool_id in TOOL_IDS {
        let items = list_tool_installations(conn, tool_id)
            .await
            .map_err(acp_error)?;
        tool_installations.extend(items);
    }
    let tool_settings = list_tool_settings(conn).await.map_err(acp_error)?;
    Ok(AgentVersionCenterSnapshot {
        catalog: catalog.view().await,
        agents,
        tool_settings,
        tool_installations,
    })
}

pub async fn refresh_core(
    conn: &DatabaseConnection,
    catalog: &CatalogStore,
) -> Result<AgentVersionCenterSnapshot, AppCommandError> {
    catalog.refresh(conn).await?;
    snapshot_core(conn, catalog).await
}

pub async fn agent_history_core(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    channel: Option<String>,
) -> Result<crate::acp::version_center::VersionHistory, AppCommandError> {
    let channel = normalize_channel(channel)?;
    AgentPlatformClient::agent_history(conn, registry::registry_id_for(agent_type), &channel).await
}

pub async fn tool_history_core(
    conn: &DatabaseConnection,
    tool_id: String,
    channel: Option<String>,
) -> Result<crate::acp::version_center::VersionHistory, AppCommandError> {
    if !known_tool(&tool_id) {
        return Err(AppCommandError::invalid_input("Unknown managed tool"));
    }
    AgentPlatformClient::tool_history(conn, &tool_id, &normalize_channel(channel)?).await
}

pub async fn set_agent_pin_core(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: Option<String>,
    channel: Option<String>,
) -> Result<(), AppCommandError> {
    let version = validate_agent_pin(conn, agent_type, version, channel).await?;
    set_agent_pin(conn, agent_type, version)
        .await
        .map_err(acp_error)
}

pub async fn set_tool_pin_core(
    conn: &DatabaseConnection,
    tool_id: String,
    version: Option<String>,
    channel: Option<String>,
) -> Result<(), AppCommandError> {
    if !known_tool(&tool_id) {
        return Err(AppCommandError::invalid_input("Unknown managed tool"));
    }
    let version = validate_tool_pin(conn, &tool_id, version, channel).await?;
    set_tool_pin(conn, &tool_id, version)
        .await
        .map_err(acp_error)
}

pub async fn install_tool_core(
    conn: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    data_dir: &std::path::Path,
    tool_id: String,
    version: Option<String>,
    channel: Option<String>,
    task_id: Option<&str>,
    emitter: Option<&crate::web::event_bridge::EventEmitter>,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    // IR-005：活跃会话存在时不切换版本，改为写入 pending activation，
    // 由会话结束后的首次启动（bootstrap_initialize）消费并激活。
    let defer_while_active = connection_manager.has_live_agent_sessions().await;
    let channel = normalize_channel(channel)?;
    install_managed_tool(
        conn,
        data_dir,
        &tool_id,
        version.as_deref(),
        &channel,
        defer_while_active,
        task_id,
        emitter,
    )
    .await
}

async fn validate_agent_pin(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: Option<String>,
    channel: Option<String>,
) -> Result<Option<String>, AppCommandError> {
    let Some(version) = normalize_version(version) else {
        return Ok(None);
    };
    let history = agent_history_core(conn, agent_type, channel).await?;
    Ok(history
        .items
        .into_iter()
        .any(|item| item.version == version && item.pinnable)
        .then_some(version)
        .map(Some)
        .ok_or_else(|| AppCommandError::invalid_input("Version cannot be pinned"))?)
}

async fn validate_tool_pin(
    conn: &DatabaseConnection,
    tool_id: &str,
    version: Option<String>,
    channel: Option<String>,
) -> Result<Option<String>, AppCommandError> {
    let Some(version) = normalize_version(version) else {
        return Ok(None);
    };
    let history = tool_history_core(conn, tool_id.to_string(), channel).await?;
    Ok(history
        .items
        .into_iter()
        .any(|item| item.version == version && item.pinnable)
        .then_some(version)
        .map(Some)
        .ok_or_else(|| AppCommandError::invalid_input("Version cannot be pinned"))?)
}

fn normalize_channel(value: Option<String>) -> Result<String, AppCommandError> {
    let value = value.unwrap_or_else(|| "stable".to_string());
    matches!(value.as_str(), "stable" | "beta")
        .then_some(value)
        .ok_or_else(|| AppCommandError::invalid_input("Invalid update channel"))
}

fn normalize_version(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn acp_error(error: AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
