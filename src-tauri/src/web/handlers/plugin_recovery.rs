use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::plugin_recovery::{self, InspectInput, RecoveryInput, RecoveryReview};
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct InspectParams {
    input: InspectInput,
}
#[derive(Deserialize)]
pub struct ApplyParams {
    input: RecoveryInput,
}

pub async fn inspect(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InspectParams>,
) -> Result<Json<RecoveryReview>, AppCommandError> {
    plugin_recovery::inspect(&state.db.conn, params.input)
        .await
        .map(Json)
}
pub async fn apply(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ApplyParams>,
) -> Result<Json<RecoveryReview>, AppCommandError> {
    plugin_recovery::apply::apply(&state.db.conn, params.input, state.emitter.clone())
        .await
        .map(Json)
}
