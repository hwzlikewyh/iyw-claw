use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::scenario_cases::{self, ScenarioCaseDetail, ScenarioCasesCatalog};
use crate::commands::scenarios::{self, ScenarioCatalog};

pub async fn catalog(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ScenarioCatalog>, AppCommandError> {
    Ok(Json(scenarios::scenarios_catalog_core(&state.db).await?))
}

pub async fn cases_catalog(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ScenarioCasesCatalog>, AppCommandError> {
    Ok(Json(
        scenario_cases::scenario_cases_catalog_core(&state.db).await?,
    ))
}

#[derive(serde::Deserialize)]
pub struct CaseDetailRequest {
    id: String,
}

pub async fn case_detail(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CaseDetailRequest>,
) -> Result<Json<ScenarioCaseDetail>, AppCommandError> {
    Ok(Json(
        scenario_cases::scenario_case_detail_core(&state.db, request.id).await?,
    ))
}
