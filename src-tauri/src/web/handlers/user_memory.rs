use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::user_memory::{
    append_user_memory_direct_core, correct_user_memory_core, delete_user_memory_candidate_core,
    get_user_memory_harvest_status_core, get_user_memory_settings_core,
    list_user_memory_candidates_core, rebuild_user_memory_candidate_index_core,
    rescan_user_memory_harvest_core, resolve_user_memory_candidate_core,
    update_user_memory_settings_core,
};
use crate::user_memory::{
    AppendUserMemoryRequest, CorrectUserMemoryRequest, CorrectUserMemoryResult,
    UserMemoryAppendResult, UserMemoryCandidateDeleteRequest, UserMemoryCandidateDeleteResult,
    UserMemoryCandidateIndexRebuildResult, UserMemoryCandidateListRequest, UserMemoryCandidatePage,
    UserMemoryCandidateResolutionResponse, UserMemoryCandidateResolveRequest,
    UserMemoryHarvestRescanResult, UserMemoryHarvestStatus, UserMemorySettingsSnapshot,
    UserMemoryUpdateRequest, UserMemoryUpdateResult,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppendUserMemoryDirectParams {
    pub request: AppendUserMemoryRequest,
}

pub async fn append_user_memory_direct(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<AppendUserMemoryDirectParams>,
) -> Result<Json<UserMemoryAppendResult>, AppCommandError> {
    Ok(Json(
        append_user_memory_direct_core(&state.user_memory, params.request).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectUserMemoryParams {
    pub request: CorrectUserMemoryRequest,
}

pub async fn correct_user_memory(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<CorrectUserMemoryParams>,
) -> Result<Json<CorrectUserMemoryResult>, AppCommandError> {
    Ok(Json(
        correct_user_memory_core(&state.user_memory, params.request).await?,
    ))
}

pub async fn get_user_memory_settings(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<UserMemorySettingsSnapshot>, AppCommandError> {
    Ok(Json(
        get_user_memory_settings_core(&state.user_memory, &state.connection_manager).await?,
    ))
}

#[derive(Deserialize)]
pub struct UpdateUserMemorySettingsParams {
    pub request: UserMemoryUpdateRequest,
}

pub async fn update_user_memory_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<UpdateUserMemorySettingsParams>,
) -> Result<Json<UserMemoryUpdateResult>, AppCommandError> {
    Ok(Json(
        update_user_memory_settings_core(
            &state.user_memory,
            &state.connection_manager,
            params.request,
        )
        .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListUserMemoryCandidatesParams {
    pub request: UserMemoryCandidateListRequest,
}

pub async fn list_user_memory_candidates(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListUserMemoryCandidatesParams>,
) -> Result<Json<UserMemoryCandidatePage>, AppCommandError> {
    Ok(Json(
        list_user_memory_candidates_core(&state.user_memory, params.request).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveUserMemoryCandidateParams {
    pub request: UserMemoryCandidateResolveRequest,
}

pub async fn resolve_user_memory_candidate(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ResolveUserMemoryCandidateParams>,
) -> Result<Json<UserMemoryCandidateResolutionResponse>, AppCommandError> {
    Ok(Json(
        resolve_user_memory_candidate_core(&state.user_memory, params.request).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteUserMemoryCandidateParams {
    pub request: UserMemoryCandidateDeleteRequest,
}

pub async fn delete_user_memory_candidate(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<DeleteUserMemoryCandidateParams>,
) -> Result<Json<UserMemoryCandidateDeleteResult>, AppCommandError> {
    Ok(Json(
        delete_user_memory_candidate_core(&state.user_memory, params.request).await?,
    ))
}

pub async fn get_user_memory_harvest_status(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<UserMemoryHarvestStatus>, AppCommandError> {
    Ok(Json(
        get_user_memory_harvest_status_core(&state.user_memory).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RescanUserMemoryHarvestParams {
    pub execute: bool,
}

pub async fn rescan_user_memory_harvest(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RescanUserMemoryHarvestParams>,
) -> Result<Json<UserMemoryHarvestRescanResult>, AppCommandError> {
    Ok(Json(
        rescan_user_memory_harvest_core(Arc::clone(&state.user_memory), params.execute).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RebuildUserMemoryCandidateIndexParams {
    pub execute: bool,
}

pub async fn rebuild_user_memory_candidate_index(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RebuildUserMemoryCandidateIndexParams>,
) -> Result<Json<UserMemoryCandidateIndexRebuildResult>, AppCommandError> {
    Ok(Json(
        rebuild_user_memory_candidate_index_core(&state.user_memory, params.execute).await?,
    ))
}
