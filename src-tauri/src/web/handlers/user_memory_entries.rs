use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::user_memory_entries::{
    apply_memory_governance_core, clear_user_memory_core, forget_user_memory_core,
    list_user_memory_entries_core, preview_memory_governance_core,
    set_user_memory_entry_status_core,
};
use crate::user_memory::{
    ApplyMemoryGovernanceRequest, ApplyMemoryGovernanceResult, ClearUserMemoryRequest,
    ClearUserMemoryResult, CloudRetrievalConfig, ForgetUserMemoryRequest, ForgetUserMemoryResult,
    MemoryGovernancePreview, RetrievalModels, SemanticPreview, SemanticStatus,
    UserMemoryEntryListRequest, UserMemoryEntryPage, UserMemoryEntryStatusRequest,
};

pub async fn get_user_memory_retrieval_models(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<RetrievalModels>, AppCommandError> {
    Ok(Json(state.user_memory.retrieval_models().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudConfigParams {
    pub config: CloudRetrievalConfig,
}

pub async fn set_user_memory_cloud_config(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<CloudConfigParams>,
) -> Result<Json<()>, AppCommandError> {
    state
        .user_memory
        .set_cloud_retrieval_config(params.config)
        .await?;
    Ok(Json(()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListEntriesParams {
    pub request: UserMemoryEntryListRequest,
}

pub async fn list_user_memory_entries(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListEntriesParams>,
) -> Result<Json<UserMemoryEntryPage>, AppCommandError> {
    Ok(Json(
        list_user_memory_entries_core(&state.user_memory, params.request).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetEntryStatusParams {
    pub request: UserMemoryEntryStatusRequest,
}

pub async fn set_user_memory_entry_status(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetEntryStatusParams>,
) -> Result<Json<()>, AppCommandError> {
    set_user_memory_entry_status_core(&state.user_memory, params.request).await?;
    Ok(Json(()))
}

pub async fn preview_memory_governance(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryGovernancePreview>, AppCommandError> {
    Ok(Json(
        preview_memory_governance_core(&state.user_memory).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyGovernanceParams {
    pub request: ApplyMemoryGovernanceRequest,
}

pub async fn apply_memory_governance(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ApplyGovernanceParams>,
) -> Result<Json<ApplyMemoryGovernanceResult>, AppCommandError> {
    Ok(Json(
        apply_memory_governance_core(&state.user_memory, params.request).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgetMemoryParams {
    pub request: ForgetUserMemoryRequest,
}

pub async fn forget_user_memory(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ForgetMemoryParams>,
) -> Result<Json<ForgetUserMemoryResult>, AppCommandError> {
    Ok(Json(
        forget_user_memory_core(&state.user_memory, params.request).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearMemoryParams {
    pub request: ClearUserMemoryRequest,
}

pub async fn clear_user_memory(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ClearMemoryParams>,
) -> Result<Json<ClearUserMemoryResult>, AppCommandError> {
    Ok(Json(
        clear_user_memory_core(&state.user_memory, params.request).await?,
    ))
}

pub async fn get_user_memory_semantic_status(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<SemanticStatus>, AppCommandError> {
    Ok(Json(state.user_memory.semantic_settings_status().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEnabledParams {
    pub enabled: bool,
}

pub async fn set_user_memory_semantic_enabled(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SemanticEnabledParams>,
) -> Result<Json<()>, AppCommandError> {
    state
        .user_memory
        .set_semantic_recall_enabled(params.enabled)
        .await?;
    Ok(Json(()))
}

pub async fn prepare_user_memory_semantic(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<SemanticStatus>, AppCommandError> {
    Ok(Json(state.user_memory.prepare_semantic_index()?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPreviewParams {
    pub query: String,
}

pub async fn preview_user_memory_semantic(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SemanticPreviewParams>,
) -> Result<Json<SemanticPreview>, AppCommandError> {
    Ok(Json(
        state
            .user_memory
            .preview_semantic_memory(params.query)
            .await?,
    ))
}
