use std::sync::Arc;

use axum::{Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::runtime_bootstrap as rb;
use crate::managed_environment::{self, ManagedEnvironmentStatusReport};

use super::{BootstrapInitializeParams, RuntimeBootstrapParams};

pub async fn runtime_bootstrap(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RuntimeBootstrapParams>,
) -> Json<rb::RuntimeBootstrapReport> {
    Json(rb::runtime_bootstrap_core(params.task_id, &state.emitter).await)
}

pub async fn bootstrap_init_status(
    Extension(_state): Extension<Arc<AppState>>,
) -> Result<Json<ManagedEnvironmentStatusReport>, AppCommandError> {
    status().await.map(Json)
}

pub async fn bootstrap_initialize(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<BootstrapInitializeParams>,
) -> Result<Json<ManagedEnvironmentStatusReport>, AppCommandError> {
    if params.repair.unwrap_or(false) {
        managed_environment::repair(&params.task_id, &state.emitter)
            .await
            .map_err(AppCommandError::task_execution_failed)?;
    }
    status().await.map(Json)
}

async fn status() -> Result<ManagedEnvironmentStatusReport, AppCommandError> {
    tokio::task::spawn_blocking(managed_environment::init_status_report)
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}
