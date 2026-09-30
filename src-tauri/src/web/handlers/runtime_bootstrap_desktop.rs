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
    let _guard = managed_environment::lock_writer().await;
    status().await.map(Json)
}

pub async fn bootstrap_initialize(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<BootstrapInitializeParams>,
) -> Result<Json<ManagedEnvironmentStatusReport>, AppCommandError> {
    let _guard = managed_environment::lock_writer().await;
    let mut report = status().await?;
    let explicit_repair = params.repair.unwrap_or(false);
    let repair = if explicit_repair {
        true
    } else if managed_environment::components_need_repair(&report) {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        report = status().await?;
        managed_environment::components_need_repair(&report)
    } else {
        false
    };
    tracing::info!(
        task_id = %params.task_id,
        repair,
        explicit_repair,
        environment_phase = %report.phase,
        unavailable_components = ?report.components.iter()
            .filter(|component| !component.active)
            .map(|component| component.component_id.as_str())
            .collect::<Vec<_>>(),
        "environment status requested"
    );
    if repair {
        managed_environment::repair_startup(&params.task_id, &state.emitter, explicit_repair)
            .await
            .map_err(AppCommandError::task_execution_failed)?;
        report = status().await?;
    }
    Ok(Json(report))
}

async fn status() -> Result<ManagedEnvironmentStatusReport, AppCommandError> {
    tokio::task::spawn_blocking(managed_environment::init_status_report)
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}
