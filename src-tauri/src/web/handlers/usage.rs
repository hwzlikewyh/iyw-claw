use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::usage;
use crate::models::UsageDashboardStats;

pub async fn get_usage_dashboard(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<UsageDashboardStats>, AppCommandError> {
    Ok(Json(usage::get_usage_dashboard_core(&state.db.conn).await?))
}
