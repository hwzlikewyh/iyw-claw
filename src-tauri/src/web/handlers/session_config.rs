use axum::Json;

use crate::commands::session_config::session_config_reconcile_diagnostics_core;
use crate::commands::session_config::SessionConfigReconcileDiagnostics;
use crate::app_error::AppCommandError;

/// Web handler：与 Tauri command 同名端点，返回最近一次对账诊断快照。
pub async fn get_session_config_reconcile_diagnostics() -> Result<Json<SessionConfigReconcileDiagnostics>, AppCommandError> {
    Ok(Json(session_config_reconcile_diagnostics_core()))
}
