use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::plugin_apps::{
    message_core, open_core, teardown_core, PluginAppMessageRequest, PluginAppMessageResponse,
    PluginAppOpenRequest, PluginAppOpenResponse, PluginAppTeardownRequest,
};

#[derive(Deserialize)]
pub struct RequestParams<T> {
    request: T,
}

pub async fn open(
    Extension(state): Extension<Arc<AppState>>,
    Json(RequestParams { request }): Json<RequestParams<PluginAppOpenRequest>>,
) -> Result<Json<PluginAppOpenResponse>, AppCommandError> {
    open_core(&state.db, &state.plugin_apps, &state.plugin_router, request)
        .await
        .map(Json)
}

pub async fn message(
    Extension(state): Extension<Arc<AppState>>,
    Json(RequestParams { request }): Json<RequestParams<PluginAppMessageRequest>>,
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
    Json(RequestParams { request }): Json<RequestParams<PluginAppTeardownRequest>>,
) -> Result<Json<()>, AppCommandError> {
    teardown_core(&state.db, &state.plugin_apps, request)
        .await
        .map(|_| Json(()))
}

pub async fn catalog(
    Extension(state): Extension<Arc<AppState>>,
    Json(RequestParams { request }): Json<
        RequestParams<crate::commands::plugin_apps::CatalogRequest>,
    >,
) -> Result<Json<Vec<crate::commands::plugin_apps::AppChoice>>, AppCommandError> {
    crate::commands::plugin_apps::catalog_core(&state.db, request)
        .await
        .map(Json)
}

pub async fn sync(
    Extension(state): Extension<Arc<AppState>>,
    Json(RequestParams { request }): Json<RequestParams<crate::commands::plugin_apps::SyncRequest>>,
) -> Result<Json<Option<crate::commands::plugin_apps::SyncUpdate>>, AppCommandError> {
    crate::commands::plugin_apps::sync_core(&state.db, &state.plugin_apps, request)
        .await
        .map(Json)
}
pub async fn launch(
    Extension(state): Extension<Arc<AppState>>,
    Json(RequestParams { request }): Json<
        RequestParams<crate::commands::plugin_apps::LaunchRequest>,
    >,
) -> Result<Json<PluginAppOpenResponse>, AppCommandError> {
    crate::commands::plugin_apps::launch_core(
        &state.db,
        (&state.plugin_apps, &state.plugin_router),
        request,
    )
    .await
    .map(Json)
}
