use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::skill_inventory::{
    self, SkillActivationSetRequest, SkillActivationSetResult, SkillInventorySnapshot,
    SkillMutationResult, SkillReconcileRequest, SkillTakeOverRequest,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInventoryListParams {
    pub workspace_path: Option<String>,
}

pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SkillInventoryListParams>,
) -> Result<Json<SkillInventorySnapshot>, AppCommandError> {
    skill_inventory::skill_inventory_list_core(&state.db.conn, params.workspace_path.as_deref())
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

pub async fn set_activation(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SkillActivationParams>,
) -> Result<Json<SkillActivationSetResult>, AppCommandError> {
    skill_inventory::skill_activation_set_core(&state.db.conn, params.request)
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

#[derive(Deserialize)]
pub struct SkillActivationParams {
    pub request: SkillActivationSetRequest,
}

pub async fn take_over(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SkillTakeOverParams>,
) -> Result<Json<SkillMutationResult>, AppCommandError> {
    skill_inventory::skill_take_over_core(&state.db.conn, params.request)
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

pub async fn reconcile(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SkillReconcileParams>,
) -> Result<Json<SkillMutationResult>, AppCommandError> {
    skill_inventory::skill_reconcile_core(&state.db.conn, params.request)
        .await
        .map(Json)
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))
}

#[derive(Deserialize)]
pub struct SkillTakeOverParams {
    pub request: SkillTakeOverRequest,
}

#[derive(Deserialize)]
pub struct SkillReconcileParams {
    pub request: SkillReconcileRequest,
}
