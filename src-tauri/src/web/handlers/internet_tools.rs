use axum::Json;
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::commands::internet_tools::{
    self as tools, AgentReachChannel, AgentReachConfigKey, InternetChannelStatus,
    InternetSkillSyncReport, InternetToolId, InternetToolInfo, InternetToolSkill,
    OpencliDoctorResult, SupportedBrowser,
};

fn map_error(error: String) -> AppCommandError {
    AppCommandError::task_execution_failed(error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolParams {
    pub tool: InternetToolId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallParams {
    pub tool: InternetToolId,
    pub remove_config: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillParams {
    pub skill_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureParams {
    pub key: AgentReachConfigKey,
    pub value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserParams {
    pub browser: SupportedBrowser,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelParams {
    pub channels: Vec<AgentReachChannel>,
}

pub async fn detect() -> Result<Json<Vec<InternetToolInfo>>, AppCommandError> {
    Ok(Json(
        tools::internet_tools_detect().await.map_err(map_error)?,
    ))
}

pub async fn install(
    Json(params): Json<ToolParams>,
) -> Result<Json<InternetToolInfo>, AppCommandError> {
    Ok(Json(
        tools::internet_tool_install(params.tool)
            .await
            .map_err(map_error)?,
    ))
}

pub async fn uninstall(
    Json(params): Json<UninstallParams>,
) -> Result<Json<InternetToolInfo>, AppCommandError> {
    Ok(Json(
        tools::internet_tool_uninstall(params.tool, params.remove_config)
            .await
            .map_err(map_error)?,
    ))
}

pub async fn sync_skills() -> Result<Json<InternetSkillSyncReport>, AppCommandError> {
    Ok(Json(
        tools::internet_tools_sync_skills()
            .await
            .map_err(map_error)?,
    ))
}

pub async fn list_skills() -> Result<Json<Vec<InternetToolSkill>>, AppCommandError> {
    Ok(Json(tools::internet_tools_list_skills().await))
}

pub async fn read_skill(Json(params): Json<SkillParams>) -> Result<Json<String>, AppCommandError> {
    Ok(Json(
        tools::internet_tools_read_skill(params.skill_id)
            .await
            .map_err(map_error)?,
    ))
}

pub async fn agent_reach_doctor() -> Result<Json<Vec<InternetChannelStatus>>, AppCommandError> {
    Ok(Json(
        tools::internet_tools_agent_reach_doctor()
            .await
            .map_err(map_error)?,
    ))
}

pub async fn opencli_doctor() -> Result<Json<OpencliDoctorResult>, AppCommandError> {
    Ok(Json(
        tools::internet_tools_opencli_doctor()
            .await
            .map_err(map_error)?,
    ))
}

pub async fn configure(Json(params): Json<ConfigureParams>) -> Result<Json<()>, AppCommandError> {
    tools::internet_tools_configure_agent_reach(params.key, params.value)
        .await
        .map_err(map_error)?;
    Ok(Json(()))
}

pub async fn import_browser(
    Json(params): Json<BrowserParams>,
) -> Result<Json<()>, AppCommandError> {
    tools::internet_tools_import_browser(params.browser)
        .await
        .map_err(map_error)?;
    Ok(Json(()))
}

pub async fn install_channels(
    Json(params): Json<ChannelParams>,
) -> Result<Json<Vec<InternetChannelStatus>>, AppCommandError> {
    Ok(Json(
        tools::internet_tools_install_channels(params.channels)
            .await
            .map_err(map_error)?,
    ))
}
