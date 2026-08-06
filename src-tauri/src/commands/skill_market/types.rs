use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::app_error::AppCommandError;
use crate::models::AgentType;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketListParams {
    pub view: String,
    pub visibility: Option<String>,
    pub publisher_type: Option<String>,
    pub category: Option<String>,
    pub q: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketCategory {
    pub key: String,
    pub fallback_name: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketVersion {
    #[serde(deserialize_with = "deserialize_id")]
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub changelog: Option<String>,
    pub status: String,
    #[serde(default)]
    pub file_count: u64,
    pub package_size: u64,
    #[serde(default)]
    pub package_type: SkillPackageType,
    #[serde(default, deserialize_with = "null_as_default")]
    pub dependencies: Vec<SkillDependency>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillPackageType {
    #[default]
    Skill,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDependency {
    #[serde(deserialize_with = "deserialize_id")]
    pub skill_id: String,
    pub slug: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDependencyInput {
    pub slug: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub mime_type: Option<String>,
}

/// Client-side install constraints for one active distribution policy.
///
/// The gateway does not resolve a compatible/incompatible verdict because it is
/// never told this build's version or os/arch. It returns the bounds and the
/// frontend compares them locally. An empty string means "unbounded".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCompatibilityConstraint {
    #[serde(default)]
    pub min_client_version: String,
    #[serde(default)]
    pub max_client_version: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketItem {
    #[serde(deserialize_with = "deserialize_id")]
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub summary: String,
    pub category: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub tags: Vec<String>,
    pub visibility: String,
    pub publisher_type: String,
    // Contract v2 fields. The gateway emits these (see internal/domain/skill
    // model.go); every one is `#[serde(default)]` so a pre-v2 gateway response
    // still deserializes. Without them declared here serde silently dropped the
    // values on the way through, and the frontend adapter fell back forever.
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub distribution_policy: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub compatibility: Vec<SkillCompatibilityConstraint>,
    pub current_version: SkillMarketVersion,
    #[serde(default)]
    pub owned_by_me: bool,
    #[serde(default)]
    pub can_manage: bool,
    #[serde(default)]
    pub installed_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketDetail {
    #[serde(flatten)]
    pub skill: SkillMarketItem,
    #[serde(default, deserialize_with = "null_as_default")]
    pub files: Vec<SkillMarketFile>,
    #[serde(default)]
    pub install_targets: Vec<AgentType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketListResult {
    #[serde(default, deserialize_with = "null_as_default")]
    pub items: Vec<SkillMarketItem>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketUploadFile {
    pub path: String,
    pub content_base64: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketPublishRequest {
    pub slug: String,
    pub display_name: String,
    pub summary: String,
    pub category: String,
    pub icon_url: Option<String>,
    pub tags: Vec<String>,
    pub visibility: String,
    pub version: String,
    pub changelog: String,
    #[serde(default)]
    pub dependencies: Vec<SkillDependencyInput>,
    pub files: Vec<SkillMarketUploadFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketMetadataRequest {
    pub id: String,
    pub display_name: String,
    pub summary: String,
    pub category: String,
    pub icon_url: Option<String>,
    pub tags: Vec<String>,
    pub visibility: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarketAddVersionRequest {
    pub id: String,
    pub version: String,
    pub changelog: String,
    #[serde(default)]
    pub dependencies: Vec<SkillDependencyInput>,
    pub files: Vec<SkillMarketUploadFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDownloadInfo {
    pub version: String,
    pub package_size: u64,
    pub content_sha256: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub object_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallPlan {
    #[serde(deserialize_with = "deserialize_id")]
    pub root_skill_id: String,
    pub root_slug: String,
    pub root_version: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub items: Vec<SkillInstallPlanItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallPlanItem {
    #[serde(deserialize_with = "deserialize_id")]
    pub skill_id: String,
    pub slug: String,
    pub display_name: String,
    pub version: String,
    pub package_type: SkillPackageType,
    pub visibility: String,
    pub publisher_type: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub dependencies: Vec<SkillDependency>,
    pub download: SkillDownloadInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTree {
    #[serde(default, deserialize_with = "null_as_default")]
    pub tree: Vec<FileNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub children: Vec<FileNode>,
}

pub fn parse_id(value: &str) -> Result<i64, AppCommandError> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| AppCommandError::invalid_input("Invalid Skill ID"))
}

pub fn parse_value<T: DeserializeOwned>(
    value: serde_json::Value,
    key: Option<&str>,
) -> Result<T, AppCommandError> {
    let selected = key.and_then(|key| value.get(key).cloned()).unwrap_or(value);
    serde_json::from_value(selected).map_err(|error| {
        AppCommandError::configuration_invalid("Invalid Skill market response")
            .with_detail(error.to_string())
    })
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn deserialize_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom(
            "Skill ID must be a string or number",
        )),
    }
}
