use crate::commands::skill_market::plugin_history::{
    self, ApplyInput, InspectInput, ListInput, VersionList, VersionReview,
};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ListParams {
    input: ListInput,
}
#[derive(Deserialize)]
pub struct InspectParams {
    input: InspectInput,
}
#[derive(Deserialize)]
pub struct ApplyParams {
    input: ApplyInput,
}

pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListParams>,
) -> Result<Json<VersionList>, AppCommandError> {
    plugin_history::list(&state.db.conn, params.input)
        .await
        .map(Json)
}
pub async fn inspect(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InspectParams>,
) -> Result<Json<VersionReview>, AppCommandError> {
    plugin_history::inspect(&state.db.conn, params.input)
        .await
        .map(Json)
}
pub async fn apply(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ApplyParams>,
) -> Result<Json<VersionReview>, AppCommandError> {
    plugin_history::apply(&state.db.conn, params.input, state.emitter.clone())
        .await
        .map(Json)
}
