use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
pub(super) struct Document {
    pub server_id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub owner: String,
    pub latest_version: String,
    pub install_options: Vec<OptionSpec>,
}

#[derive(Clone, Deserialize)]
pub(super) struct OptionSpec {
    pub id: String,
    pub label: String,
    pub protocol: String,
    #[serde(default)]
    pub description: String,
    pub spec: Value,
    #[serde(default)]
    pub parameters: Vec<Field>,
    #[serde(default)]
    pub artifact_id: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub auth_mode: String,
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub protocol_versions: Vec<String>,
}

#[derive(Clone, Deserialize)]
pub(super) struct Field {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub default_value: Option<Value>,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub enum_values: Vec<String>,
}
