use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::log_report::{self, ReportContext, ReportRequest, ReportResult};

#[derive(Deserialize)]
pub struct ContextParams {
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub struct SubmitParams {
    pub request: ReportRequest,
}

pub async fn get_log_report_context(
    Json(params): Json<ContextParams>,
) -> Result<Json<ReportContext>, AppCommandError> {
    log_report::get_log_report_context_core(params.date)
        .await
        .map(Json)
}

pub async fn submit_log_report(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SubmitParams>,
) -> Result<Json<ReportResult>, AppCommandError> {
    log_report::submit_log_report_core(&state.db.conn, params.request)
        .await
        .map(Json)
}
