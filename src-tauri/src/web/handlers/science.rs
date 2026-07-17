use axum::Json;
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::commands::experts::ExpertInstallStatus;
use crate::commands::science::{self, ScienceListItem};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScienceIdParams {
    pub skill_id: String,
}

pub async fn list() -> Result<Json<Vec<ScienceListItem>>, AppCommandError> {
    science::science_list().await.map(Json).map_err(map_error)
}

pub async fn list_all_install_statuses() -> Result<Json<Vec<ExpertInstallStatus>>, AppCommandError>
{
    science::science_list_all_install_statuses()
        .await
        .map(Json)
        .map_err(map_error)
}

pub async fn read_content(
    Json(params): Json<ScienceIdParams>,
) -> Result<Json<String>, AppCommandError> {
    science::science_read_content(params.skill_id)
        .await
        .map(Json)
        .map_err(map_error)
}

pub async fn open_central_dir() -> Result<Json<String>, AppCommandError> {
    science::science_open_central_dir()
        .await
        .map(Json)
        .map_err(map_error)
}

fn map_error(error: science::ScienceError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
