use serde::{Deserialize, Serialize};

use crate::acp::manager::ConnectionManager;
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
#[cfg(feature = "tauri-runtime")]
use tauri::State;

const SETTINGS_KEY: &str = "acp.max_idle_connections";
pub const DEFAULT_MAX_IDLE_CONNECTIONS: usize = 4;

/// User preference for completed, recoverable agent processes kept resident.
/// `None` means no count cap; memory-pressure protection remains active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleAgentSettings {
    pub max_idle_connections: Option<usize>,
}

impl Default for IdleAgentSettings {
    fn default() -> Self {
        Self {
            max_idle_connections: Some(DEFAULT_MAX_IDLE_CONNECTIONS),
        }
    }
}

pub async fn get_idle_agent_settings_core(
    db: &sea_orm::DatabaseConnection,
) -> Result<IdleAgentSettings, AppCommandError> {
    let raw = app_metadata_service::get_value(db, SETTINGS_KEY)
        .await
        .map_err(AppCommandError::from)?;
    match raw {
        Some(value) => serde_json::from_str(&value).map_err(|error| {
            AppCommandError::configuration_invalid("Invalid idle agent settings")
                .with_detail(error.to_string())
        }),
        None => Ok(IdleAgentSettings::default()),
    }
}

pub async fn set_idle_agent_settings_core(
    db: &sea_orm::DatabaseConnection,
    manager: &ConnectionManager,
    settings: IdleAgentSettings,
) -> Result<IdleAgentSettings, AppCommandError> {
    let serialized = serde_json::to_string(&settings).map_err(|error| {
        AppCommandError::invalid_input("Invalid idle agent settings").with_detail(error.to_string())
    })?;
    app_metadata_service::upsert_value(db, SETTINGS_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)?;
    tracing::info!(
        max_idle_connections = ?settings.max_idle_connections,
        "[ACP] idle agent preference updated"
    );
    let reclaimed = manager
        .sweep_excess_idle(settings.max_idle_connections)
        .await;
    if reclaimed > 0 {
        tracing::info!(
            reclaimed,
            "[ACP] idle agent preference reclaimed connections"
        );
    }
    Ok(settings)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_idle_agent_settings(
    db: State<'_, AppDatabase>,
) -> Result<IdleAgentSettings, AppCommandError> {
    get_idle_agent_settings_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_idle_agent_settings(
    settings: IdleAgentSettings,
    db: State<'_, AppDatabase>,
    manager: State<'_, ConnectionManager>,
) -> Result<IdleAgentSettings, AppCommandError> {
    set_idle_agent_settings_core(&db.conn, &manager, settings).await
}
