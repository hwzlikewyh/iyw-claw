#[cfg(feature = "tauri-runtime")]
use crate::app_error::AppCommandError;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
#[cfg(feature = "tauri-runtime")]
use crate::system_skills::{self, SystemSkillsUpdateState};
#[cfg(feature = "tauri-runtime")]
use crate::web::event_bridge::EventEmitter;

#[cfg(feature = "tauri-runtime")]
fn context(app: tauri::AppHandle) -> (std::path::PathBuf, EventEmitter) {
    (system_skills::data_dir_from_env(), EventEmitter::Tauri(app))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn system_skills_update_state(
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let (data_dir, emitter) = context(app);
    system_skills::snapshot_core(&db.conn, &data_dir, &emitter).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn system_skills_check_update(
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let (data_dir, emitter) = context(app);
    system_skills::check_update_core(&db.conn, &data_dir, &emitter).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn system_skills_apply_update(
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let (data_dir, emitter) = context(app);
    system_skills::apply_update_core(&db.conn, &data_dir, &emitter).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn system_skills_set_auto_update(
    enabled: bool,
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let (_, emitter) = context(app);
    system_skills::set_auto_update_core(&db.conn, enabled, &emitter).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn system_skills_rollback(
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let (data_dir, emitter) = context(app);
    system_skills::rollback_core(&db.conn, &data_dir, &emitter).await
}
