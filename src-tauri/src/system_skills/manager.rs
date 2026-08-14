use std::path::Path;

use chrono::Utc;
use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

use super::state::{self, SystemSkillsUpdateLifecycle, SystemSkillsUpdateState};

fn bundled_state(
    emitter: &EventEmitter,
    checked: bool,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    let version = format!("v{}", super::manifest::embedded_version()?);
    Ok(state::mutate(emitter, |value| {
        value.status = SystemSkillsUpdateLifecycle::UpToDate;
        value.current_version = Some(version.clone());
        value.current_commit = None;
        value.previous_version = None;
        value.latest_version = Some(version);
        if checked {
            value.last_checked_at = Some(Utc::now().to_rfc3339());
        }
        value.dirty = false;
        value.error = None;
    }))
}

pub async fn snapshot_core(
    _conn: &DatabaseConnection,
    _data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    bundled_state(emitter, false)
}

pub async fn check_update_core(
    _conn: &DatabaseConnection,
    _data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    bundled_state(emitter, true)
}

pub async fn apply_update_core(
    _conn: &DatabaseConnection,
    _data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    bundled_state(emitter, true)
}

pub async fn rollback_core(
    _conn: &DatabaseConnection,
    _data_dir: &Path,
    emitter: &EventEmitter,
) -> Result<SystemSkillsUpdateState, AppCommandError> {
    bundled_state(emitter, true)
}

pub async fn startup_update_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    emitter: &EventEmitter,
) {
    match snapshot_core(conn, data_dir, emitter).await {
        Ok(snapshot) => tracing::debug!(
            target: "system_skills",
            version = ?snapshot.current_version,
            "bundled system skills are active"
        ),
        Err(error) => tracing::warn!(
            target: "system_skills",
            "bundled system skill state load failed: {error}"
        ),
    }
}
