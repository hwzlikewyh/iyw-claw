use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::user_memory::{
    get_user_memory_settings_core, update_user_memory_settings_core,
};
use crate::user_memory::{
    UserMemorySettingsSnapshot, UserMemoryUpdateRequest, UserMemoryUpdateResult,
};

pub async fn get_user_memory_settings(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<UserMemorySettingsSnapshot>, AppCommandError> {
    Ok(Json(
        get_user_memory_settings_core(&state.user_memory, &state.connection_manager).await?,
    ))
}

#[derive(Deserialize)]
pub struct UpdateUserMemorySettingsParams {
    pub request: UserMemoryUpdateRequest,
}

pub async fn update_user_memory_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<UpdateUserMemorySettingsParams>,
) -> Result<Json<UserMemoryUpdateResult>, AppCommandError> {
    Ok(Json(
        update_user_memory_settings_core(
            &state.user_memory,
            &state.connection_manager,
            params.request,
        )
        .await?,
    ))
}
