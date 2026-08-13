use crate::acp::manager::ConnectionManager;
use crate::acp::version_center::{CatalogStore, ManagedToolInstallResult};
use crate::app_error::AppCommandError;
use crate::commands::agent_version_center::{
    agent_history_core, install_tool_core, refresh_core, set_agent_pin_core, set_tool_pin_core,
    snapshot_core, tool_history_core, AgentHistoryRequest, AgentPinRequest, AgentRollbackRequest,
    AgentVersionRequest, ToolHistoryRequest, ToolInstallRequest, ToolPinRequest,
};
use crate::commands::agent_version_operations::{
    install_agent_version_core, rollback_agent_version_core, switch_agent_version_core,
    AgentVersionOperationResult,
};
use crate::db::AppDatabase;
use crate::web::event_bridge::EventEmitter;

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
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let emitter = EventEmitter::Tauri(app);
    // Emit started so the frontend knows to show progress UI
    crate::web::event_bridge::emit_event(
        &emitter,
        "app://agent-install",
        crate::commands::acp::AgentInstallEvent {
            task_id: task_id.clone(),
            kind: crate::commands::acp::AgentInstallEventKind::Started,
            payload: String::new(),
        },
    );
    let result = install_tool_core(
        &db.conn,
        &connection_manager,
        &crate::system_skills::data_dir_from_env(),
        request.tool_id,
        request.version,
        request.channel,
        Some(&task_id),
        Some(&emitter),
    )
    .await;
    match &result {
        Ok(r) => crate::web::event_bridge::emit_event(
            &emitter,
            "app://agent-install",
            crate::commands::acp::AgentInstallEvent {
                task_id: task_id.clone(),
                kind: crate::commands::acp::AgentInstallEventKind::Completed,
                payload: if r.deferred {
                    format!(
                        "{} v{} installed, activation deferred (active session)",
                        r.tool_id, r.version
                    )
                } else {
                    format!("{} v{} installed", r.tool_id, r.version)
                },
            },
        ),
        Err(e) => crate::web::event_bridge::emit_event(
            &emitter,
            "app://agent-install",
            crate::commands::acp::AgentInstallEvent {
                task_id: task_id.clone(),
                kind: crate::commands::acp::AgentInstallEventKind::Failed,
                payload: e.to_string(),
            },
        ),
    }
    result
}

#[tauri::command]
pub async fn agent_version_center_install_agent(
    request: AgentVersionRequest,
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    install_agent_version_core(
        &db,
        &connection_manager,
        &EventEmitter::Tauri(app),
        request.agent_type,
        request.version,
    )
    .await
}

#[tauri::command]
pub async fn agent_version_center_switch_agent(
    request: AgentVersionRequest,
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    switch_agent_version_core(
        &db.conn,
        &connection_manager,
        &EventEmitter::Tauri(app),
        request.agent_type,
        request.version,
    )
    .await
}

#[tauri::command]
pub async fn agent_version_center_rollback_agent(
    request: AgentRollbackRequest,
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<AgentVersionOperationResult, AppCommandError> {
    rollback_agent_version_core(
        &db.conn,
        &connection_manager,
        &EventEmitter::Tauri(app),
        request.agent_type,
    )
    .await
}
