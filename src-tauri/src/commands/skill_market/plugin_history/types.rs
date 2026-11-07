use super::super::plugin_install_context::PreparedPluginInstall;
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use crate::models::AgentType;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInput {
    pub market_skill_id: String,
    #[serde(default = "default_page")]
    pub page: u32,
}

fn default_page() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectInput {
    pub market_skill_id: String,
    pub version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub market_skill_id: String,
    pub version: String,
    pub review_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionList {
    pub slug: String,
    pub current_version: String,
    pub versions: Vec<VersionEntry>,
    pub next_page: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub version: String,
    pub recorded_at: Option<String>,
    pub current: bool,
    pub available: bool,
    pub issue: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionReview {
    pub slug: String,
    pub current_version: String,
    pub version: String,
    pub state: String,
    pub review_token: Option<String>,
    pub scope_changed: bool,
    pub permissions_changed: bool,
    pub preparation_changed: bool,
    pub listed: Option<bool>,
    pub issues: Vec<String>,
    pub dependencies: Vec<DependencyChange>,
    pub related: Vec<RelatedCapability>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedCapability {
    pub package_type: String,
    pub id: String,
    pub slug: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyChange {
    pub package_type: String,
    pub id: String,
    pub slug: String,
    pub version: String,
    pub installed_version: Option<String>,
    pub changing: bool,
    pub managed: bool,
    pub ready: bool,
}

pub(super) struct Prepared {
    pub current: PluginInstallationRecord,
    pub scope: String,
    pub view: VersionReview,
    pub agents: Vec<AgentType>,
    pub plugins: Vec<PreparedPluginInstall>,
    pub skills: Vec<crate::commands::acp::MarketSkillInstall>,
}
