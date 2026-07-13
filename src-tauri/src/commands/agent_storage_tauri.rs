use std::path::{Path, PathBuf};

use crate::acp::agent_storage::{validate_root, RootValidation, STORAGE_ROOT_ENV};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::agent_storage::{
    get_agent_storage_status_core, initialize_agent_storage_core, migrate_agent_storage_core,
    update_agent_profile_override_core, AgentStorageStatus, MigrationActivity,
};

fn system_drive() -> Option<String> {
    std::env::var("SystemDrive").ok()
}

#[tauri::command]
pub async fn migrate_agent_storage(
    db: tauri::State<'_, crate::db::AppDatabase>,
    manager: tauri::State<'_, crate::acp::manager::ConnectionManager>,
    root: String,
    allow_system_drive: bool,
) -> Result<AgentStorageStatus, AppCommandError> {
    let _migration_guard = crate::acp::agent_storage_work::try_begin_agent_storage_migration()
        .ok_or_else(|| {
            AppCommandError::task_execution_failed(
                "Stop active Agent installation tasks before migrating storage",
            )
        })?;
    let activity = MigrationActivity {
        active_connections: !manager.list_connections().await.is_empty(),
        active_installs: false,
    };
    migrate_agent_storage_core(
        &db.conn,
        PathBuf::from(root.trim()),
        allow_system_drive,
        system_drive().as_deref(),
        activity,
    )
    .await
}

#[tauri::command]
pub async fn get_agent_storage_status(
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<AgentStorageStatus, AppCommandError> {
    let executable = std::env::current_exe().map_err(AppCommandError::io)?;
    get_agent_storage_status_core(
        &db.conn,
        &executable,
        std::env::var_os(STORAGE_ROOT_ENV),
        None,
    )
    .await
}

#[tauri::command]
pub async fn validate_agent_storage_root(root: String) -> Result<RootValidation, AppCommandError> {
    Ok(validate_root(
        Path::new(root.trim()),
        system_drive().as_deref(),
    ))
}

#[tauri::command]
pub async fn initialize_agent_storage(
    db: tauri::State<'_, crate::db::AppDatabase>,
    root: String,
    allow_system_drive: bool,
    import_existing_settings: Option<bool>,
) -> Result<AgentStorageStatus, AppCommandError> {
    initialize_agent_storage_core(
        &db.conn,
        PathBuf::from(root.trim()),
        allow_system_drive,
        system_drive().as_deref(),
        import_existing_settings.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn update_agent_profile_override(
    db: tauri::State<'_, crate::db::AppDatabase>,
    agent_type: AgentType,
    path: Option<String>,
    allow_system_drive: bool,
    allow_user_global_profile: Option<bool>,
) -> Result<AgentStorageStatus, AppCommandError> {
    let path = path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    update_agent_profile_override_core(
        &db.conn,
        agent_type,
        path,
        allow_system_drive,
        allow_user_global_profile.unwrap_or(false),
        system_drive().as_deref(),
    )
    .await
}
