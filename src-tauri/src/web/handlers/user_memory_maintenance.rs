use crate::user_memory::{
    MemoryMaintenanceStatus, MemoryMigrationPreview, ReconcileMemoryMigrationRequest,
    ReconcileMemoryMigrationResult, ResolveMemoryReviewRequest,
};
use crate::{app_error::AppCommandError, app_state::AppState};
use axum::{extract::Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_user_memory_maintenance(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryMaintenanceStatus>, AppCommandError> {
    Ok(Json(state.user_memory.memory_maintenance_status().await?))
}

pub async fn run_user_memory_maintenance(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<()>, AppCommandError> {
    state.user_memory.run_memory_maintenance().await?;
    Ok(Json(()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveParams {
    request: ResolveMemoryReviewRequest,
}

pub async fn resolve_user_memory_review(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ResolveParams>,
) -> Result<Json<()>, AppCommandError> {
    state
        .user_memory
        .resolve_memory_review(params.request)
        .await?;
    Ok(Json(()))
}

pub async fn preview_user_memory_migration(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<MemoryMigrationPreview>, AppCommandError> {
    Ok(Json(state.user_memory.preview_memory_migration().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileMigrationParams {
    request: ReconcileMemoryMigrationRequest,
}

pub async fn reconcile_user_memory_migration(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ReconcileMigrationParams>,
) -> Result<Json<ReconcileMemoryMigrationResult>, AppCommandError> {
    Ok(Json(
        state
            .user_memory
            .reconcile_memory_migration(params.request)
            .await?,
    ))
}
