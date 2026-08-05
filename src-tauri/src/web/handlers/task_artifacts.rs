use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::task_artifacts::{list_task_artifacts_core, ListTaskArtifactsParams};
use crate::db::service::task_artifact_service::TaskArtifactInfo;

pub async fn list_task_artifacts(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListTaskArtifactsParams>,
) -> Result<Json<Vec<TaskArtifactInfo>>, AppCommandError> {
    Ok(Json(
        list_task_artifacts_core(&state.db.conn, params.conversation_id, params.folder_id).await?,
    ))
}
