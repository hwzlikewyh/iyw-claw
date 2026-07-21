use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::user_memory::{
    delete_user_memory_candidate_core, get_user_memory_settings_core,
    list_user_memory_candidates_core, resolve_user_memory_candidate_core,
    update_user_memory_settings_core,
};
use crate::user_memory::{
    UserMemoryCandidateDeleteRequest, UserMemoryCandidateDeleteResult,
    UserMemoryCandidateListRequest, UserMemoryCandidatePage, UserMemoryCandidateResolutionResponse,
    UserMemoryCandidateResolveRequest, UserMemorySettingsSnapshot, UserMemoryUpdateRequest,
    UserMemoryUpdateResult,
};

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
