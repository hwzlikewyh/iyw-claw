use crate::user_memory::{
    ActivateMemoryAuthorityRequest, MemoryAuthorityStatus, MemoryEffectivenessStatus,
    MemoryRevisionEntry, RecordMemoryRecallFeedbackRequest,
};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{extract::Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_user_memory_authority(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryAuthorityStatus>, AppCommandError> {
    Ok(Json(state.user_memory.memory_authority_status().await?))
}

pub async fn prepare_user_memory_authority(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryAuthorityStatus>, AppCommandError> {
    Ok(Json(state.user_memory.prepare_memory_authority().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateParams {
    request: ActivateMemoryAuthorityRequest,
}

pub async fn activate_user_memory_authority(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ActivateParams>,
) -> Result<Json<MemoryAuthorityStatus>, AppCommandError> {
    Ok(Json(
        state
            .user_memory
            .activate_memory_authority(params.request)
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryParams {
    id: String,
}

pub async fn get_user_memory_history(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<HistoryParams>,
) -> Result<Json<Vec<MemoryRevisionEntry>>, AppCommandError> {
    Ok(Json(
        state.user_memory.memory_revision_history(params.id).await?,
    ))
}

pub async fn get_user_memory_receipts(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<HistoryParams>,
) -> Result<Json<Vec<crate::user_memory::MemoryRecallReceipt>>, AppCommandError> {
    Ok(Json(
        state.user_memory.memory_recall_receipts(params.id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackParams {
    request: RecordMemoryRecallFeedbackRequest,
}

pub async fn record_memory_recall_feedback(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<FeedbackParams>,
) -> Result<Json<()>, AppCommandError> {
    state
        .user_memory
        .record_memory_recall_feedback(params.request)
        .await?;
    Ok(Json(()))
}

pub async fn get_memory_effectiveness(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryEffectivenessStatus>, AppCommandError> {
    Ok(Json(state.user_memory.memory_effectiveness_status().await?))
}
