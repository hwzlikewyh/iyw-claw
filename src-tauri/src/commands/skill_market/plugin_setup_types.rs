use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Parameter {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub placeholder: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolAction {
    pub tool_name: String,
    pub arguments: Map<String, Value>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Authentication {
    pub status: ToolAction,
    pub authenticated_pointer: String,
    #[serde(default)]
    pub login: Option<ToolAction>,
    #[serde(default)]
    pub authorization_url_pointer: String,
    #[serde(default)]
    pub logout: Option<ToolAction>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preparation {
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub authentication: Option<Authentication>,
}
