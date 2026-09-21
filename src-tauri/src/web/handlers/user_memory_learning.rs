use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::user_memory::{BackgroundLearningConfig, BackgroundLearningStatus};
use axum::{extract::Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_user_memory_learning(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<BackgroundLearningStatus>, AppCommandError> {
    Ok(Json(state.user_memory.background_learning_status().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningParams {
    pub config: BackgroundLearningConfig,
}

pub async fn set_user_memory_learning(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<LearningParams>,
) -> Result<Json<()>, AppCommandError> {
    state
        .user_memory
        .set_background_learning(params.config)
        .await?;
    Ok(Json(()))
}

pub async fn refresh_user_memory_views(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<usize>, AppCommandError> {
    Ok(Json(state.user_memory.refresh_generated_views().await?))
}
