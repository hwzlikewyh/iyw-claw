use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginManifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub components: Vec<SkillPluginComponent>,
    #[serde(default)]
    pub bindings: Vec<SkillPluginBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginComponent {
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub server_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SkillPluginBinding {
    pub skill_key: String,
    pub connector_key: String,
}
