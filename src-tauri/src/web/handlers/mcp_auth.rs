use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::{mcp_check, mcp_cli};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerParams {
    server_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginParams {
    server_id: String,
    terminal_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelParams {
    terminal_id: String,
}

pub async fn check(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ServerParams>,
) -> Result<Json<mcp_check::ConnectorCheck>, AppCommandError> {
    mcp_check::check(&state.db.conn, &params.server_id)
        .await
        .map(Json)
}

pub async fn begin(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<BeginParams>,
) -> Result<Json<()>, AppCommandError> {
    mcp_cli::begin(
        &state.db.conn,
        (&params.server_id, &params.terminal_id, "web".into()),
        (&state.terminal_manager, state.emitter.clone()),
    )
    .await
    .map(Json)
}

pub async fn cancel(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<CancelParams>,
) -> Result<Json<()>, AppCommandError> {
    mcp_cli::cancel(&params.terminal_id, &state.terminal_manager).map(Json)
}

pub async fn logout(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ServerParams>,
) -> Result<Json<()>, AppCommandError> {
    mcp_cli::logout(&state.db.conn, &params.server_id)
        .await
        .map(Json)
}
