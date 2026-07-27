use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::performance;

pub async fn get_performance_stats(
    Extension(_state): Extension<Arc<AppState>>,
) -> Result<Json<performance::SystemPerformanceStats>, AppCommandError> {
    let stats = performance::get_performance_stats_core().await;
    Ok(Json(stats))
}
