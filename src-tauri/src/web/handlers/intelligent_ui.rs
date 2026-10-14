use crate::commands::intelligent_ui::{self as core, UiSettings, UiState, UiStateInput};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{extract::Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SettingsParams {
    pub settings: UiSettings,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadParams {
    pub conversation_id: i32,
}
#[derive(Deserialize)]
pub struct SaveParams {
    pub input: UiStateInput,
}

pub async fn settings(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<UiSettings>, AppCommandError> {
    Ok(Json(core::settings_core(&state.db.conn).await?))
}
pub async fn set_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SettingsParams>,
) -> Result<Json<UiSettings>, AppCommandError> {
    Ok(Json(
        core::set_settings_core(&state.db.conn, &state.emitter, params.settings).await?,
    ))
}
pub async fn read(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ReadParams>,
) -> Result<Json<Vec<UiState>>, AppCommandError> {
    Ok(Json(
        core::read_states_core(&state.db.conn, params.conversation_id).await?,
    ))
}
pub async fn save(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SaveParams>,
) -> Result<Json<UiState>, AppCommandError> {
    Ok(Json(
        core::save_state_core(&state.db.conn, &state.emitter, params.input).await?,
    ))
}
