use serde::{Deserialize, Deserializer, Serialize};

/// Accept `null` where a list is expected, yielding the empty list.
///
/// Go marshals a nil slice to `null`, not `[]`, so any server-side list that
/// happens to be empty arrives as `null`. Plain `#[serde(default)]` only covers
/// a *missing* key, so `null` would fail with "invalid type: null, expected a
/// sequence" and take down the whole response. An empty catalog is a legitimate
/// state, so it must not break startup.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub schema_version: u32,
    pub revision: u64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub generated_at: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub platforms: Vec<CatalogPlatform>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub tools: Vec<CatalogTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPlatform {
    pub registry_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub status: String,
    pub sort_order: i32,
    pub channel: String,
    #[serde(default)]
    pub recommended_version: String,
    #[serde(default)]
    pub minimum_safe_version: String,
    pub default_update_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTool {
    pub tool_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub status: String,
    pub sort_order: i32,
    pub channel: String,
    #[serde(default)]
    pub recommended_version: String,
    #[serde(default)]
    pub minimum_safe_version: String,
    pub default_update_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionHistory {
    #[serde(default, deserialize_with = "null_as_default")]
    pub items: Vec<VersionHistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionHistoryItem {
    pub id: String,
    pub version: String,
    pub title: String,
    #[serde(default)]
    pub notes_markdown: String,
    pub channel: String,
    pub lifecycle_status: String,
    pub security_status: String,
    pub update_policy: String,
    #[serde(default)]
    pub published_at: Option<String>,
    pub rollout_eligible: bool,
    pub recommended: bool,
    pub minimum_safe: bool,
    pub pinnable: bool,
    #[serde(default)]
    pub delivery_kind: String,
    #[serde(default)]
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOffer {
    pub revision: u64,
    pub registry_id: String,
    pub version_id: String,
    pub version: String,
    pub channel: String,
    pub security_status: String,
    pub selection_reason: String,
    pub effective_update_policy: String,
    pub required: bool,
    pub delivery: AgentDelivery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDelivery {
    pub kind: String,
    pub runtime: String,
    pub target: String,
    pub arch: String,
    pub recipe_schema_version: u32,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub components: Vec<DeliveryComponent>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub origins: Vec<DeliveryOrigin>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub tool_requirements: Vec<ToolRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryComponent {
    pub component_key: String,
    pub package_name: String,
    pub package_version: String,
    #[serde(default)]
    pub registry_integrity: String,
    pub source_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryOrigin {
    pub source_key: String,
    pub source_kind: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequirement {
    pub tool_id: String,
    pub minimum_version: String,
    #[serde(default)]
    pub maximum_version: String,
    #[serde(default)]
    pub maximum_inclusive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOffer {
    pub revision: u64,
    pub tool_id: String,
    pub version_id: String,
    pub version: String,
    pub channel: String,
    pub security_status: String,
    pub selection_reason: String,
    pub effective_update_policy: String,
    pub required: bool,
    pub artifact: ToolArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolArtifact {
    pub id: String,
    pub runtime: String,
    pub target: String,
    pub arch: String,
    pub package_kind: String,
    pub size: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTicket {
    pub url: String,
    pub expires_at: String,
    pub file_name: String,
    pub content_type: String,
    pub size: i64,
    pub sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveAgentRequest<'a> {
    pub registry_id: &'a str,
    pub current_version: &'a str,
    pub requested_version: Option<&'a str>,
    pub pinned_version: Option<&'a str>,
    pub client_version: &'a str,
    pub runtime: &'a str,
    pub target: &'a str,
    pub arch: &'a str,
    pub channel: &'a str,
    pub reason: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveToolRequest<'a> {
    pub tool_id: &'a str,
    pub current_version: &'a str,
    pub requested_version: Option<&'a str>,
    pub pinned_version: Option<&'a str>,
    pub client_version: &'a str,
    pub runtime: &'a str,
    pub target: &'a str,
    pub arch: &'a str,
    pub channel: &'a str,
    pub reason: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest<'a> {
    pub registry_id: Option<&'a str>,
    pub tool_id: Option<&'a str>,
    pub version_id: &'a str,
    pub artifact_id: &'a str,
    pub catalog_revision: u64,
    pub client_version: &'a str,
    pub runtime: &'a str,
    pub target: &'a str,
    pub arch: &'a str,
    pub channel: &'a str,
}

#[cfg(test)]
mod tests {
    use super::{CatalogSnapshot, VersionHistory};

    /// The exact payload a Go server sends when the catalog is empty: a nil
    /// slice marshals to `null`, not `[]`. Rejecting it failed the whole
    /// startup refresh with "Invalid Agent platform response data".
    #[test]
    fn catalog_accepts_null_lists() {
        let snapshot: CatalogSnapshot = serde_json::from_str(
            r#"{"schemaVersion":1,"revision":2,"generatedAt":null,"platforms":null,"tools":null}"#,
        )
        .expect("null lists should decode as empty");
        assert_eq!(snapshot.revision, 2);
        assert!(snapshot.platforms.is_empty());
        assert!(snapshot.tools.is_empty());
        assert!(snapshot.generated_at.is_empty());
    }

    #[test]
    fn catalog_accepts_missing_lists() {
        let snapshot: CatalogSnapshot = serde_json::from_str(r#"{"schemaVersion":1,"revision":3}"#)
            .expect("absent lists should decode as empty");
        assert!(snapshot.platforms.is_empty());
        assert!(snapshot.tools.is_empty());
    }

    #[test]
    fn catalog_still_decodes_populated_lists() {
        let snapshot: CatalogSnapshot = serde_json::from_str(
            r#"{"schemaVersion":1,"revision":4,"platforms":[],"tools":[{
                "toolId":"node","displayName":"Node.js","status":"active","sortOrder":1,
                "channel":"stable","recommendedVersion":"24.18.1",
                "defaultUpdatePolicy":"required"}]}"#,
        )
        .expect("populated lists should decode");
        assert_eq!(snapshot.tools.len(), 1);
        assert_eq!(snapshot.tools[0].tool_id, "node");
        assert_eq!(snapshot.tools[0].recommended_version, "24.18.1");
        assert!(snapshot.tools[0].minimum_safe_version.is_empty());
    }

    #[test]
    fn history_accepts_null_items() {
        let history: VersionHistory =
            serde_json::from_str(r#"{"items":null}"#).expect("null items should decode as empty");
        assert!(history.items.is_empty());
    }
}
