use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveRequest {
    pub schema_version: u8,
    pub installation_id: String,
    pub client_version: String,
    pub pc_version: String,
    pub channel: &'static str,
    pub runtime: &'static str,
    pub target: String,
    pub arch: String,
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEntry {
    pub component_key: String,
    pub component_kind: String,
    pub current_version: String,
    pub sha256: String,
    pub active: bool,
    pub pinned: bool,
    pub healthy: bool,
    pub lkg: bool,
}

#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub data: Option<T>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPlan {
    pub plan_id: String,
    pub catalog_revision: u64,
    pub binding_revision: u64,
    pub pc_version: String,
    pub target: String,
    pub arch: String,
    pub actions: Vec<EnvironmentAction>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentAction {
    pub component_id: String,
    pub component_kind: String,
    pub optional: bool,
    pub action: String,
    pub version: String,
    pub artifact: EnvironmentArtifact,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentArtifact {
    pub version_id: String,
    pub artifact_id: String,
    pub package_kind: String,
    pub file_name: String,
    #[serde(rename = "size")]
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledComponent {
    pub component_id: String,
    pub component_kind: String,
    pub optional: bool,
    pub version: String,
    pub version_id: String,
    pub artifact_id: String,
    pub package_kind: String,
    pub artifact_sha256: String,
    pub relative_path: String,
    pub entrypoints: BTreeMap<String, String>,
    pub files: Vec<FileRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSnapshot {
    pub schema_version: u8,
    pub generation: String,
    pub pc_version: String,
    pub catalog_revision: u64,
    pub binding_revision: u64,
    pub target: String,
    pub arch: String,
    pub created_at: String,
    pub components: Vec<InstalledComponent>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedState {
    pub transaction_id: String,
    pub plan_id: String,
    pub pc_version: String,
    pub catalog_revision: u64,
    pub binding_revision: u64,
    pub target: String,
    pub arch: String,
    pub created_at: String,
    pub components: Vec<PreparedComponent>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedComponent {
    pub component: InstalledComponent,
    pub staged: bool,
}
