use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::mcp_install::{self, InstallInput, InstallTask};

#[derive(Deserialize)]
pub struct BeginParams {
    input: InstallInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskParams {
    task_id: String,
}

pub async fn begin(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<BeginParams>,
) -> Result<Json<InstallTask>, AppCommandError> {
    let view = mcp_install::begin(&state.db.conn, params.input).await?;
    let id = view.id.clone();
    tokio::spawn(async move {
        mcp_install::wait_for_completion(&id).await;
        crate::commands::mcp::refresh_running_sessions(
            &state.connection_manager,
            &state.db,
            &state.data_dir,
        )
        .await;
    });
    Ok(Json(view))
}

pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<InstallTask>>, AppCommandError> {
    mcp_install::list(&state.db.conn).await.map(Json)
}

pub async fn status(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<TaskParams>,
) -> Result<Json<InstallTask>, AppCommandError> {
    mcp_install::status(&state.db.conn, &params.task_id)
        .await
        .map(Json)
}

pub async fn cancel(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<TaskParams>,
) -> Result<Json<InstallTask>, AppCommandError> {
    mcp_install::cancel(&state.db.conn, &params.task_id)
        .await
        .map(Json)
}
