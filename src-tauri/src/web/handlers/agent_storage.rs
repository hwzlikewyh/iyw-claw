use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::acp::agent_storage::{RootValidation, STORAGE_ROOT_ENV};
use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::agent_storage::{
    get_agent_storage_status_core, initialize_agent_storage_core, migrate_agent_storage_core,
    update_agent_profile_override_core, AgentStorageStatus, MigrationActivity,
};
use crate::models::agent::AgentType;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRootParams {
    pub root: String,
    #[serde(default)]
    pub allow_system_drive: bool,
}

pub async fn migrate_agent_storage(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<StorageRootParams>,
) -> Result<Json<AgentStorageStatus>, AppCommandError> {
    let _migration_guard = crate::acp::agent_storage_work::try_begin_agent_storage_migration()
        .ok_or_else(|| {
            AppCommandError::task_execution_failed(
                "Stop active Agent installation tasks before migrating storage",
            )
        })?;
    let activity = MigrationActivity {
        active_connections: !state.connection_manager.list_connections().await.is_empty(),
        active_installs: false,
    };
    let status = migrate_agent_storage_core(
        &state.db.conn,
        PathBuf::from(params.root.trim()),
        params.allow_system_drive,
        system_drive().as_deref(),
        activity,
    )
    .await?;
    Ok(Json(status))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeStorageParams {
    pub root: String,
    #[serde(default)]
    pub allow_system_drive: bool,
    #[serde(default)]
    pub import_existing_settings: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOverrideParams {
    pub agent_type: AgentType,
    pub path: Option<String>,
    #[serde(default)]
    pub allow_system_drive: bool,
    #[serde(default)]
    pub allow_user_global_profile: bool,
}

fn system_drive() -> Option<String> {
    std::env::var("SystemDrive").ok()
}

pub async fn get_agent_storage_status(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<AgentStorageStatus>, AppCommandError> {
    let executable = std::env::current_exe().map_err(AppCommandError::io)?;
    let status = get_agent_storage_status_core(
        &state.db.conn,
        &executable,
        std::env::var_os(STORAGE_ROOT_ENV),
        None,
    )
    .await?;
    Ok(Json(status))
}

pub async fn validate_agent_storage_root(
    Json(params): Json<StorageRootParams>,
) -> Result<Json<RootValidation>, AppCommandError> {
    Ok(Json(crate::acp::agent_storage::validate_root(
        Path::new(params.root.trim()),
        system_drive().as_deref(),
    )))
}

pub async fn initialize_agent_storage(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InitializeStorageParams>,
) -> Result<Json<AgentStorageStatus>, AppCommandError> {
    let status = initialize_agent_storage_core(
        &state.db.conn,
        PathBuf::from(params.root.trim()),
        params.allow_system_drive,
        system_drive().as_deref(),
        params.import_existing_settings,
    )
    .await?;
    Ok(Json(status))
}

pub async fn update_agent_profile_override(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ProfileOverrideParams>,
) -> Result<Json<AgentStorageStatus>, AppCommandError> {
    let path = params
        .path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let status = update_agent_profile_override_core(
        &state.db.conn,
        params.agent_type,
        path,
        params.allow_system_drive,
        params.allow_user_global_profile,
        system_drive().as_deref(),
    )
    .await?;
    Ok(Json(status))
}
