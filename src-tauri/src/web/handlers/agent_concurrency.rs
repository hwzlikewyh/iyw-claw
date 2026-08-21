//! HTTP mirror for the independent Agent concurrency setting.

use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::agent_concurrency::{
    get_agent_concurrency_settings_core, set_agent_concurrency_settings_core,
    AgentConcurrencySettings,
};

pub async fn get_agent_concurrency_settings(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<AgentConcurrencySettings>, AppCommandError> {
    Ok(Json(
        get_agent_concurrency_settings_core(&state.db.conn).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SetAgentConcurrencySettingsParams {
    pub settings: AgentConcurrencySettings,
}

pub async fn set_agent_concurrency_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetAgentConcurrencySettingsParams>,
) -> Result<Json<AgentConcurrencySettings>, AppCommandError> {
    Ok(Json(
        set_agent_concurrency_settings_core(
            &state.db.conn,
            &state.delegation_broker,
            params.settings,
        )
        .await?,
    ))
}
