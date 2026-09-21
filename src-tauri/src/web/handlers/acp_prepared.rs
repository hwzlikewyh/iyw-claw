use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::acp::prepared_session::{PrepareSessionRequest, PreparedSessionHandle};
use crate::app_error::AppCommandError;
use crate::app_state::AppState;

#[derive(Deserialize)]
pub struct PrepareParams {
    request: PrepareSessionRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandleParams {
    preparation_id: String,
}

pub async fn prepare(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<PrepareParams>,
) -> Result<Json<Option<PreparedSessionHandle>>, AppCommandError> {
    crate::commands::acp_prepared::prepare_core(
        &state.connection_manager,
        &state.db,
        (params.request, "web".into(), state.emitter.clone()),
    )
    .await
    .map(Json)
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

pub async fn cancel(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<HandleParams>,
) -> Json<()> {
    state
        .connection_manager
        .cancel_preparation(&params.preparation_id, "web")
        .await;
    Json(())
}

pub async fn reserve_workspace(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<HandleParams>,
) -> Json<Option<String>> {
    Json(
        state
            .connection_manager
            .reserve_prepared_workspace(&params.preparation_id, "web")
            .await,
    )
}
