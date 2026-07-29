use crate::acp::manager::ConnectionManager;
use crate::acp::version_center::{CatalogStore, ManagedToolInstallResult};
use crate::app_error::AppCommandError;
use crate::commands::agent_version_center::{
    agent_history_core, install_tool_core, refresh_core, set_agent_pin_core, set_tool_pin_core,
    snapshot_core, tool_history_core, AgentHistoryRequest, AgentPinRequest, ToolHistoryRequest,
    ToolInstallRequest, ToolPinRequest,
};
use crate::db::AppDatabase;

#[tauri::command]
pub async fn agent_version_center_snapshot(
    db: tauri::State<'_, AppDatabase>,
    catalog: tauri::State<'_, CatalogStore>,
) -> Result<crate::commands::agent_version_center::AgentVersionCenterSnapshot, AppCommandError> {
    snapshot_core(&db.conn, &catalog).await
}

#[tauri::command]
pub async fn agent_version_center_refresh(
    db: tauri::State<'_, AppDatabase>,
    catalog: tauri::State<'_, CatalogStore>,
) -> Result<crate::commands::agent_version_center::AgentVersionCenterSnapshot, AppCommandError> {
    refresh_core(&db.conn, &catalog).await
}

#[tauri::command]
pub async fn agent_version_center_agent_history(
    request: AgentHistoryRequest,
    db: tauri::State<'_, AppDatabase>,
) -> Result<crate::acp::version_center::VersionHistory, AppCommandError> {
    agent_history_core(&db.conn, request.agent_type, request.channel).await
}

#[tauri::command]
pub async fn agent_version_center_tool_history(
    request: ToolHistoryRequest,
    db: tauri::State<'_, AppDatabase>,
) -> Result<crate::acp::version_center::VersionHistory, AppCommandError> {
    tool_history_core(&db.conn, request.tool_id, request.channel).await
}

#[tauri::command]
pub async fn agent_version_center_set_agent_pin(
    request: AgentPinRequest,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    set_agent_pin_core(
        &db.conn,
        request.agent_type,
        request.version,
        request.channel,
    )
    .await
}

#[tauri::command]
pub async fn agent_version_center_set_tool_pin(
    request: ToolPinRequest,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    set_tool_pin_core(&db.conn, request.tool_id, request.version, request.channel).await
}

#[tauri::command]
pub async fn agent_version_center_install_tool(
    request: ToolInstallRequest,
    db: tauri::State<'_, AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    install_tool_core(
        &db.conn,
        &connection_manager,
        &crate::system_skills::data_dir_from_env(),
        request.tool_id,
        request.version,
        request.channel,
    )
    .await
}
