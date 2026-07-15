use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::managed_skills::{
    self, ManagedSkillFamily, ManagedSkillFamilyState, ManagedSkillGlobalState,
    ManagedSkillSyncReport,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyParams {
    pub family: ManagedSkillFamily,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalEnabledParams {
    pub family: ManagedSkillFamily,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSkillEnabledParams {
    pub family: ManagedSkillFamily,
    pub skill_id: String,
    pub enabled: bool,
}

pub async fn managed_skills_get_global_state(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ManagedSkillGlobalState>, AppCommandError> {
    Ok(Json(
        managed_skills::get_global_state_core(&state.db.conn).await?,
    ))
}

pub async fn managed_skills_set_global_enabled(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetGlobalEnabledParams>,
) -> Result<Json<ManagedSkillSyncReport>, AppCommandError> {
    Ok(Json(
        managed_skills::set_global_enabled_core(&state.db.conn, params.family, params.enabled)
            .await?,
    ))
}

pub async fn managed_skills_get_family_state(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<FamilyParams>,
) -> Result<Json<ManagedSkillFamilyState>, AppCommandError> {
    Ok(Json(
        managed_skills::get_family_state_core(&state.db.conn, params.family).await?,
    ))
}

pub async fn managed_skills_set_skill_enabled(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetSkillEnabledParams>,
) -> Result<Json<ManagedSkillSyncReport>, AppCommandError> {
    Ok(Json(
        managed_skills::set_skill_enabled_core(
            &state.db.conn,
            params.family,
            params.skill_id,
            params.enabled,
        )
        .await?,
    ))
}
