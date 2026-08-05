use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::skill_market::{
    self, SkillMarketAddVersionRequest, SkillMarketCategory, SkillMarketDetail,
    SkillMarketListParams, SkillMarketListResult, SkillMarketMetadataRequest,
    SkillMarketPublishRequest, SkillMarketVersion,
};
use crate::models::AgentType;

#[derive(Deserialize)]
pub struct ListParams {
    params: SkillMarketListParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailParams {
    id: String,
    version: Option<String>,
}

#[derive(Deserialize)]
pub struct IDParams {
    id: String,
}

#[derive(Deserialize)]
pub struct PublishParams {
    request: SkillMarketPublishRequest,
}

#[derive(Deserialize)]
pub struct AddVersionParams {
    request: SkillMarketAddVersionRequest,
}

#[derive(Deserialize)]
pub struct MetadataParams {
    request: SkillMarketMetadataRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallParams {
    id: String,
    version: String,
    agent_types: Vec<AgentType>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildParams {
    id: String,
    version: String,
}

pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListParams>,
) -> Result<Json<SkillMarketListResult>, AppCommandError> {
    Ok(Json(
        skill_market::list_core(&state.db.conn, params.params).await?,
    ))
}

pub async fn categories(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<SkillMarketCategory>>, AppCommandError> {
    Ok(Json(skill_market::categories_core(&state.db.conn).await?))
}

pub async fn detail(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<DetailParams>,
) -> Result<Json<SkillMarketDetail>, AppCommandError> {
    Ok(Json(
        skill_market::detail_core(&state.db.conn, params.id, params.version).await?,
    ))
}

pub async fn versions(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<IDParams>,
) -> Result<Json<Vec<SkillMarketVersion>>, AppCommandError> {
    Ok(Json(
        skill_market::versions_core(&state.db.conn, params.id).await?,
    ))
}

pub async fn publish(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<PublishParams>,
) -> Result<Json<SkillMarketDetail>, AppCommandError> {
    Ok(Json(
        skill_market::publish_core(&state.db.conn, params.request).await?,
    ))
}

pub async fn add_version(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<AddVersionParams>,
) -> Result<Json<SkillMarketDetail>, AppCommandError> {
    Ok(Json(
        skill_market::add_version_core(&state.db.conn, params.request).await?,
    ))
}

pub async fn update_metadata(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<MetadataParams>,
) -> Result<Json<SkillMarketDetail>, AppCommandError> {
    Ok(Json(
        skill_market::update_metadata_core(&state.db.conn, params.request).await?,
    ))
}

pub async fn delete(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<IDParams>,
) -> Result<Json<()>, AppCommandError> {
    skill_market::delete_core(&state.db.conn, params.id).await?;
    Ok(Json(()))
}

pub async fn install(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InstallParams>,
) -> Result<Json<()>, AppCommandError> {
    skill_market::install_core(
        &state.db.conn,
        params.id,
        params.version,
        params.agent_types,
    )
    .await?;
    Ok(Json(()))
}

pub async fn uninstall(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<IDParams>,
) -> Result<Json<()>, AppCommandError> {
    skill_market::uninstall_core(&state.db.conn, params.id).await?;
    Ok(Json(()))
}

pub async fn rebuild_artifact(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RebuildParams>,
) -> Result<Json<SkillMarketVersion>, AppCommandError> {
    Ok(Json(
        skill_market::rebuild_artifact_core(&state.db.conn, params.id, params.version).await?,
    ))
}
