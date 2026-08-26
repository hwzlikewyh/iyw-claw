use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginManifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub targets: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub components: Vec<SkillPluginComponent>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub bindings: Vec<SkillPluginBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<SkillPluginPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginComponent {
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginBinding {
    pub skill_key: String,
    pub connector_key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillPluginPermissions {
    pub workspace: SkillPluginWorkspacePermissions,
    pub network: SkillPluginNetworkPermissions,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub host: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillPluginWorkspacePermissions {
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub read: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillPluginNetworkPermissions {
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub connect_domains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub resource_domains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_null_vec")]
    pub frame_domains: Vec<String>,
}

pub(super) fn deserialize_null_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
