use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageInputMode {
    Native,
    Fallback,
    #[default]
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tool_calling: bool,
    #[serde(default)]
    pub parallel_tool_calling: bool,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub audio_input: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub prompt_cache: bool,
    #[serde(default)]
    pub image_generation: bool,
    #[serde(default)]
    pub image_editing: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLimits {
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_input_tokens: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelCapabilitySnapshot {
    pub capabilities: ModelCapabilities,
    pub image_input_mode: ImageInputMode,
    pub limits: ModelLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeCatalog {
    pub ids: Vec<&'static str>,
    pub capabilities: HashMap<&'static str, ModelCapabilitySnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PersistedCatalogV2 {
    pub version: u32,
    pub models: Vec<PersistedModel>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PersistedModel {
    pub id: String,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub image_input_mode: ImageInputMode,
    #[serde(default)]
    pub limits: ModelLimits,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum PersistedCatalog {
    Legacy(Vec<String>),
    Current(PersistedCatalogV2),
}
