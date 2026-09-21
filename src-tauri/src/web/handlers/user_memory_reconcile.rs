use crate::user_memory::{
    MemoryReconciliation, MemoryReconciliationResult, ResolveMemoryFileRequest,
    RestoreMemoryAuthorityRequest,
};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{extract::Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_user_memory_reconciliation(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryReconciliation>, AppCommandError> {
    Ok(Json(state.user_memory.memory_reconciliation().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileParams {
    request: ResolveMemoryFileRequest,
}

pub async fn resolve_user_memory_file(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<FileParams>,
) -> Result<Json<MemoryReconciliationResult>, AppCommandError> {
    Ok(Json(
        state
            .user_memory
            .resolve_memory_file(params.request)
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreParams {
    request: RestoreMemoryAuthorityRequest,
}

pub async fn restore_user_memory_authority(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RestoreParams>,
) -> Result<Json<MemoryReconciliationResult>, AppCommandError> {
    Ok(Json(
        state
            .user_memory
            .restore_memory_authority(params.request)
            .await?,
    ))
}
