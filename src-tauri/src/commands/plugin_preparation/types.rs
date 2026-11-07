use serde::{Deserialize, Serialize};

use crate::commands::skill_market::SkillPluginPermissions;
use crate::plugin_runtime::dependencies::DependencyReport;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadInput {
    pub market_skill_id: String,
    pub conversation_id: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeInput {
    pub market_skill_id: String,
    pub conversation_id: i32,
    pub context_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationView {
    pub slug: String,
    pub installed_version: String,
    pub schema_version: u32,
    pub state: String,
    pub issues: Vec<String>,
    pub dependencies: DependencyReport,
    pub permissions: Option<SkillPluginPermissions>,
    pub permission_granted: bool,
    pub connectors: Vec<ConnectorState>,
    pub context_token: Option<String>,
    pub workspace_name: Option<String>,
    pub agent_type: Option<crate::models::AgentType>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorState {
    pub key: String,
    pub enabled: bool,
    pub configuration_ready: bool,
    pub parameters: Vec<ParameterState>,
    pub has_authentication: bool,
    pub can_login: bool,
    pub can_logout: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterState {
    #[serde(flatten)]
    pub declaration: crate::commands::skill_market::plugin_setup::Parameter,
    pub value: Option<String>,
    pub configured: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationInput {
    pub market_skill_id: String,
    pub conversation_id: i32,
    pub context_token: String,
    pub connector_key: String,
    pub values: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub clear_keys: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub state: String,
    pub tool_count: usize,
}
