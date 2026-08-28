use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::plugin_apps::{
    message_core, open_core, teardown_core, PluginAppMessageRequest, PluginAppMessageResponse,
    PluginAppOpenRequest, PluginAppOpenResponse, PluginAppTeardownRequest,
};

pub async fn open(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<PluginAppOpenRequest>,
) -> Result<Json<PluginAppOpenResponse>, AppCommandError> {
    open_core(&state.db, &state.plugin_apps, &state.plugin_router, request)
        .await
        .map(Json)
}

pub async fn message(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<PluginAppMessageRequest>,
) -> Result<Json<PluginAppMessageResponse>, AppCommandError> {
    message_core(
        &state.db,
        &state.plugin_apps,
        &state.plugin_router,
        &state.connection_manager,
        request,
    )
    .await
    .map(Json)
}

pub async fn teardown(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<PluginAppTeardownRequest>,
) -> Result<Json<()>, AppCommandError> {
    teardown_core(&state.db, &state.plugin_apps, request)
        .await
        .map(|_| Json(()))
}
