use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::task_artifacts::{list_task_artifacts_core, ListTaskArtifactsParams};
use crate::db::service::task_artifact_service::TaskArtifactPage;

pub async fn list_task_artifacts(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListTaskArtifactsParams>,
) -> Result<Json<TaskArtifactPage>, AppCommandError> {
    Ok(Json(
        list_task_artifacts_core(
            &state.db.conn,
            params.conversation_id,
            params.message_id,
            params.folder_id,
            params.latest_turn_only.unwrap_or(false),
            params.search,
            params.page,
            params.page_size,
        )
        .await?,
    ))
}
