use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::scenarios::{self, ScenarioCatalog};

pub async fn catalog(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ScenarioCatalog>, AppCommandError> {
    Ok(Json(scenarios::scenarios_catalog_core(&state.db).await?))
}
