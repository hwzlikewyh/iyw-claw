use serde::Deserialize;

use crate::models::agent::AgentType;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHistoryRequest {
    pub agent_type: AgentType,
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolHistoryRequest {
    pub tool_id: String,
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPinRequest {
    pub agent_type: AgentType,
    pub version: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVersionRequest {
    pub agent_type: AgentType,
    pub version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRollbackRequest {
    pub agent_type: AgentType,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPinRequest {
    pub tool_id: String,
    pub version: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallRequest {
    pub tool_id: String,
    pub version: Option<String>,
    pub channel: Option<String>,
}
