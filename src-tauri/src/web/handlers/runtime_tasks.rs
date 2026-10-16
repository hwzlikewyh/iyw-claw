use crate::acp::runtime_tasks::{control::TaskControl, RuntimeTaskSnapshot};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskParams {
    connection_id: Option<String>,
    conversation_id: Option<i32>,
    request: Option<TaskControl>,
}

pub async fn snapshot(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<TaskParams>,
) -> Result<Json<RuntimeTaskSnapshot>, AppCommandError> {
    state
        .connection_manager
        .runtime_tasks_for(params.connection_id.as_deref(), params.conversation_id)
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

pub async fn control(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<TaskParams>,
) -> Result<Json<RuntimeTaskSnapshot>, AppCommandError> {
    let request = params
        .request
        .ok_or_else(|| AppCommandError::task_execution_failed("Task control request is missing"))?;
    let connection = params.connection_id.ok_or_else(|| {
        AppCommandError::task_execution_failed("A live connection is required for task control")
    })?;
    state
        .connection_manager
        .control_runtime_task(&connection, request)
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailParams {
    connection_id: String,
    task_id: String,
}

pub async fn detail(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<DetailParams>,
) -> Result<Json<crate::models::ConversationDetail>, AppCommandError> {
    state
        .connection_manager
        .runtime_task_detail(&params.connection_id, &params.task_id)
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}
