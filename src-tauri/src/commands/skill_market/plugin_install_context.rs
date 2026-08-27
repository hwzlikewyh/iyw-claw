use sea_orm::DatabaseConnection;

use crate::acp::skill_package::ValidatedSkillPackage;
use crate::commands::acp::{MarketSkillInstall, MarketSkillMarker};
use crate::models::AgentType;

use super::plugin_manifest::ValidatedPluginPackage;

pub(super) struct PreparedPluginInstall {
    pub(super) market_skill_id: i64,
    pub(super) slug: String,
    pub(super) version: String,
    pub(super) object_sha256: String,
    pub(super) publisher_id: String,
    pub(super) signature_key_id: String,
    pub(super) package: ValidatedSkillPackage,
    pub(super) plugin: ValidatedPluginPackage,
    pub(super) marker: MarketSkillMarker,
}

pub(super) struct MarketInstallPlanExecution<'a> {
    pub(super) conn: &'a DatabaseConnection,
    pub(super) agent_types: &'a [AgentType],
    pub(super) root_skill_id: i64,
    pub(super) skill_installs: Vec<MarketSkillInstall>,
    pub(super) plugins: Vec<PreparedPluginInstall>,
}
