use crate::commands::skill_market::plugin_remove::{self, ApplyInput, InspectInput, RemovalReview};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;
#[derive(Deserialize)]
pub struct InspectParams {
    input: InspectInput,
}
#[derive(Deserialize)]
pub struct ApplyParams {
    input: ApplyInput,
}
pub async fn inspect(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InspectParams>,
) -> Result<Json<RemovalReview>, AppCommandError> {
    plugin_remove::inspect(&state.db.conn, params.input)
        .await
        .map(Json)
}
pub async fn apply(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ApplyParams>,
) -> Result<Json<RemovalReview>, AppCommandError> {
    plugin_remove::apply(&state.db.conn, params.input, state.emitter.clone())
        .await
        .map(Json)
}
pub async fn status(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InspectParams>,
) -> Result<Json<Option<RemovalReview>>, AppCommandError> {
    plugin_remove::status(&state.db.conn, params.input)
        .await
        .map(Json)
}
pub async fn cleanup(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InspectParams>,
) -> Result<Json<RemovalReview>, AppCommandError> {
    plugin_remove::retry_cleanup(&state.db.conn, params.input)
        .await
        .map(Json)
}

pub async fn recover(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InspectParams>,
) -> Result<Json<RemovalReview>, AppCommandError> {
    plugin_remove::recover(&state.db.conn, params.input, state.emitter.clone())
        .await
        .map(Json)
}
