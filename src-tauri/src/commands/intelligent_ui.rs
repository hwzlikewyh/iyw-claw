//! 生成 UI 的开关、状态与两种运行模式共用入口。
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::web::event_bridge::{emit_event, EventEmitter};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

pub mod context;
mod fences;
mod store;
mod validation;
pub use store::{read_states_core, save_state_core, UiState, UiStateInput};

pub const SETTINGS_EVENT: &str = "intelligent-ui://settings";
pub const STATE_EVENT: &str = "intelligent-ui://state";
const SETTINGS_KEY: &str = "intelligent_ui.enabled";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiSettings {
    pub enabled: bool,
}

pub async fn settings_core(conn: &DatabaseConnection) -> Result<UiSettings, AppCommandError> {
    let enabled = app_metadata_service::get_value(conn, SETTINGS_KEY)
        .await?
        .is_some_and(|raw| raw == "true");
    Ok(UiSettings { enabled })
}

pub async fn set_settings_core(
    conn: &DatabaseConnection,
    emitter: &EventEmitter,
    settings: UiSettings,
) -> Result<UiSettings, AppCommandError> {
    app_metadata_service::upsert_value(
        conn,
        SETTINGS_KEY,
        if settings.enabled { "true" } else { "false" },
    )
    .await?;
    tracing::info!(
        enabled = settings.enabled,
        "[IntelligentUI] settings changed"
    );
    emit_event(emitter, SETTINGS_EVENT, settings.clone());
    Ok(settings)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_intelligent_ui_settings(
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<UiSettings, AppCommandError> {
    settings_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_intelligent_ui_settings(
    settings: UiSettings,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<UiSettings, AppCommandError> {
    set_settings_core(&db.conn, &EventEmitter::Tauri(app), settings).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn read_intelligent_ui_states(
    conversation_id: i32,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<Vec<UiState>, AppCommandError> {
    read_states_core(&db.conn, conversation_id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn save_intelligent_ui_state(
    input: UiStateInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<UiState, AppCommandError> {
    save_state_core(&db.conn, &EventEmitter::Tauri(app), input).await
}
