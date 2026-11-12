use crate::commands::skill_market::SkillPluginManifest;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryPlan {
    pub root_skill_id: String,
    pub root_version: String,
    pub items: Vec<RecoveryItem>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryItem {
    pub skill_id: String,
    pub slug: String,
    pub version: String,
    pub package_type: String,
    pub visibility: String,
    pub publisher_type: String,
    pub content_sha256: String,
    #[serde(default)]
    pub dependencies: Vec<RecoveryDependency>,
    pub plugin: Option<SkillPluginManifest>,
    pub listed: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryDependency {
    pub skill_id: String,
    pub slug: String,
    pub version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryInput {
    pub market_skill_id: String,
    pub review_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectInput {
    pub market_skill_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReview {
    pub slug: String,
    pub version: String,
    pub state: String,
    pub scope_changed: bool,
    pub reset_permissions: bool,
    pub credentials_retained: bool,
    pub listed: Option<bool>,
    pub review_token: Option<String>,
    pub issues: Vec<String>,
    pub dependencies: Vec<DependencyView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyView {
    pub id: String,
    pub slug: String,
    pub required_version: String,
    pub installed_version: Option<String>,
    pub ready: bool,
}

pub(super) struct Inspection {
    pub record: crate::db::service::plugin_installation_service::PluginInstallationRecord,
    pub account_scope: String,
    pub plan: Option<RecoveryPlan>,
    pub view: RecoveryReview,
    pub targets: Vec<RecoveryTarget>,
}

pub(super) struct RecoveryTarget {
    pub record: crate::db::service::plugin_installation_service::PluginInstallationRecord,
    pub previous_pointer: Option<Vec<u8>>,
    pub pointer_path: std::path::PathBuf,
    pub metadata: RecoveryItem,
    pub scope_changed: bool,
}
