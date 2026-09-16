use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::usage;
use crate::models::UsageDashboardStats;

#[derive(Default, Deserialize)]
pub struct UsageQuery {
    days: Option<usize>,
}

pub async fn get_usage_dashboard(
    Extension(state): Extension<Arc<AppState>>,
    payload: Option<Json<UsageQuery>>,
) -> Result<Json<UsageDashboardStats>, AppCommandError> {
    let query = payload.map(|Json(query)| query).unwrap_or_default();
    Ok(Json(
        usage::get_usage_dashboard_core(&state.db.conn, query.days).await?,
    ))
}
