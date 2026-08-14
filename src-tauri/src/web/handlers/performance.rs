use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::idle_agent_settings;
use crate::commands::performance;

pub async fn get_performance_stats(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<performance::AppPerformanceStats>, AppCommandError> {
    let stats =
        performance::get_performance_stats_core(&state.connection_manager, &state.db.conn).await;
    Ok(Json(stats))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetIdleAgentSettingsParams {
    pub settings: idle_agent_settings::IdleAgentSettings,
}

pub async fn get_idle_agent_settings(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<idle_agent_settings::IdleAgentSettings>, AppCommandError> {
    Ok(Json(
        idle_agent_settings::get_idle_agent_settings_core(&state.db.conn).await?,
    ))
}

pub async fn set_idle_agent_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetIdleAgentSettingsParams>,
) -> Result<Json<idle_agent_settings::IdleAgentSettings>, AppCommandError> {
    Ok(Json(
        idle_agent_settings::set_idle_agent_settings_core(
            &state.db.conn,
            &state.connection_manager,
            params.settings,
        )
        .await?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndAgentRuntimeSessionParams {
    connection_id: String,
}

pub async fn end_agent_runtime_session(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<EndAgentRuntimeSessionParams>,
) -> Result<Json<bool>, AppCommandError> {
    let ended = performance::end_agent_runtime_session_core(
        &state.connection_manager,
        &params.connection_id,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(ended))
}
