use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectInput {
    pub market_skill_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub market_skill_id: String,
    pub review_token: String,
    pub clear_account_data: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalReview {
    pub id: String,
    pub slug: String,
    pub version: String,
    pub state: String,
    pub review_token: Option<String>,
    pub affected: Vec<Capability>,
    pub retained: Vec<Retained>,
    pub warnings: Vec<String>,
    pub clear_available: bool,
    pub parameter_records: usize,
    pub legacy_records_retained: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub id: String,
    pub slug: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Retained {
    pub id: String,
    pub slug: String,
    pub shared: bool,
}

pub(super) struct Prepared {
    pub root: PluginInstallationRecord,
    pub scope: String,
    pub records: Vec<PluginInstallationRecord>,
    pub pointers: Vec<PointerChange>,
    pub view: RemovalReview,
    pub cleanup: super::cleanup::Plan,
}
pub(super) struct PointerChange {
    pub path: PathBuf,
    pub previous: Vec<u8>,
    pub next: Vec<u8>,
}
