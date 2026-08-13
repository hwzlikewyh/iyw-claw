mod agents;
mod tools;

use crate::models::agent::AgentType;

pub use agents::{
    activate_agent, list_agent_installations, promote_agent_lkg, record_agent_ready, recover_agent,
    set_agent_pin,
};
pub use tools::{
    activate_tool, list_tool_installations, list_tool_settings, record_tool_ready, set_tool_pin,
};

pub type AgentInstallation = crate::db::entities::agent_installation::Model;
pub type ManagedToolInstallation = crate::db::entities::managed_tool_installation::Model;
pub type ManagedToolSetting = crate::db::entities::managed_tool_setting::Model;

pub const STATUS_READY: &str = "ready";
pub const STATUS_ACTIVE: &str = "active";
pub const ORIGIN_MANAGED: &str = "managed";

#[derive(Debug, Clone)]
pub struct ReadyAgentInstallation<'a> {
    pub agent_type: AgentType,
    pub registry_id: &'a str,
    pub version: &'a str,
    pub delivery_kind: &'a str,
    pub artifact_id: Option<&'a str>,
    pub source_key: Option<&'a str>,
    pub expected_sha256: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ReadyToolInstallation<'a> {
    pub tool_id: &'a str,
    pub version: &'a str,
    pub runtime: &'a str,
    pub target: &'a str,
    pub arch: &'a str,
    pub origin: &'a str,
    pub artifact_id: Option<&'a str>,
    pub expected_sha256: Option<&'a str>,
}

pub(super) fn database_error(error: sea_orm::DbErr) -> crate::acp::error::AcpError {
    crate::acp::error::AcpError::protocol(error.to_string())
}

pub(super) fn serialize_agent_type(
    agent_type: AgentType,
) -> Result<String, crate::acp::error::AcpError> {
    serde_json::to_string(&agent_type)
        .map_err(|error| crate::acp::error::AcpError::protocol(error.to_string()))
}
