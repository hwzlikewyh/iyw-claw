use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::system_skills::{self, SystemSkillsUpdateState};

#[derive(Deserialize)]
pub struct SetAutoUpdateParams {
    pub enabled: bool,
}

pub async fn state(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<SystemSkillsUpdateState>, AppCommandError> {
    Ok(Json(
        system_skills::snapshot_core(&state.db.conn, &state.data_dir, &state.emitter).await?,
    ))
}

pub async fn check_update(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<SystemSkillsUpdateState>, AppCommandError> {
    Ok(Json(
        system_skills::check_update_core(&state.db.conn, &state.data_dir, &state.emitter).await?,
    ))
}

pub async fn apply_update(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<SystemSkillsUpdateState>, AppCommandError> {
    Ok(Json(
        system_skills::apply_update_core(&state.db.conn, &state.data_dir, &state.emitter).await?,
    ))
}

pub async fn set_auto_update(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetAutoUpdateParams>,
) -> Result<Json<SystemSkillsUpdateState>, AppCommandError> {
    Ok(Json(
        system_skills::set_auto_update_core(&state.db.conn, params.enabled, &state.emitter).await?,
    ))
}

pub async fn rollback(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<SystemSkillsUpdateState>, AppCommandError> {
    Ok(Json(
        system_skills::rollback_core(&state.db.conn, &state.data_dir, &state.emitter).await?,
    ))
}
