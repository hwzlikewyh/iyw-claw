//! Shared model metadata types exchanged between Codex services and clients.
//!
//! These types are serialized across core, TUI, app-server, and SDK boundaries, so field defaults
//! are used to preserve compatibility when older payloads omit newly introduced attributes.

use std::fmt;
use std::str::FromStr;

use chrono::DateTime;
use chrono::Utc;
use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::InstanceType;
use schemars::schema::Metadata;
use schemars::schema::Schema;
use schemars::schema::SchemaObject;
use schemars::schema::StringValidation;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de::DeserializeOwned;
use serde::de::Error;
use strum_macros::Display;
use strum_macros::EnumIter;
use tracing::warn;
use ts_rs::TS;

use crate::config_types::Personality;
use crate::config_types::ReasoningSummary;
use crate::config_types::SERVICE_TIER_DEFAULT_REQUEST_VALUE;
use crate::config_types::ServiceTier;
use crate::config_types::Verbosity;
use crate::protocol::MultiAgentVersion;

#[path = "openai_models/guardian_v2.rs"]
mod guardian_v2;

pub use guardian_v2::GuardianV2ModelConfig;
pub use guardian_v2::GuardianV2TranscriptModelConfig;

const PERSONALITY_PLACEHOLDER: &str = "{{ personality }}";
/// Backend model-catalog specialty identifying cybersecurity-focused models.
pub const MODEL_SPECIALTY_CYBER: &str = "cyber";
pub const SPEED_TIER_FAST: &str = "fast";

/// See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#get-started-with-reasoning
#[derive(Debug, Default, Clone, PartialEq, Eq, TS, Hash)]
#[ts(type = "string")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
    Max,
    Ultra,
    Persistent,
    /// A model-defined effort value that this client does not know yet.
    Custom(String),
}

impl ReasoningEffort {
    /// Returns the exact value used on the wire.
    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
            Self::Persistent => "persistent",
            Self::Custom(effort) => effort,
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl JsonSchema for ReasoningEffort {
    fn schema_name() -> String {
        "ReasoningEffort".to_string()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            metadata: Some(Box::new(Metadata {
                description: Some(
                    "A non-empty reasoning effort value advertised by the model.".to_string(),
                ),
                ..Default::default()
            })),
            string: Some(Box::new(StringValidation {
                min_length: Some(1),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

impl Serialize for ReasoningEffort {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ReasoningEffort {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let effort = String::deserialize(deserializer)?;
        effort.parse().map_err(D::Error::custom)
    }
}

impl FromStr for ReasoningEffort {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::XHigh),
            "max" => Ok(Self::Max),
            "ultra" => Ok(Self::Ultra),
            "persistent" => Ok(Self::Persistent),
            "" => Err("reasoning_effort must not be empty".to_string()),
            effort => Ok(Self::Custom(effort.to_string())),
        }
    }
}

/// Canonical user-input modality tags advertised by a model.
#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Display,
    JsonSchema,
    TS,
    EnumIter,
    Hash,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum InputModality {
    /// Plain text turns and tool payloads.
    Text,
    /// Image attachments included in user turns.
    Image,
    /// Audio attachments included in user turns.
    Audio,
}

/// Backward-compatible default when `input_modalities` is omitted on the wire.
///
/// Legacy payloads predate modality metadata, so we conservatively assume both text and images are
/// accepted unless a preset explicitly narrows support.
pub fn default_input_modalities() -> Vec<InputModality> {
    vec![InputModality::Text, InputModality::Image]
}

/// A reasoning effort option that can be surfaced for a model.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
pub struct ReasoningEffortPreset {
    /// Effort level that the model supports.
    pub effort: ReasoningEffort,
    /// Short human description shown next to the effort in UIs.
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct ModelUpgrade {
    pub id: String,
    pub migration_config_key: String,
    pub model_link: Option<String>,
    pub upgrade_copy: Option<String>,
    pub migration_markdown: Option<String>,
    /// Informational time when the model associated with this upgrade is scheduled to retire.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_rfc3339_timestamp"
    )]
    #[schemars(with = "Option<String>")]
    #[ts(type = "string", optional)]
    pub retirement_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
pub struct ModelAvailabilityNux {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
pub struct ModelServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Metadata describing a Codex-supported model.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct ModelPreset {
    /// Stable identifier for the preset.
    pub id: String,
    /// Model slug (e.g., "gpt-5").
    pub model: String,
    /// Display name shown in UIs.
    pub display_name: String,
    /// Short human description shown in UIs.
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_specialty: Option<String>,
    /// Reasoning effort applied when none is explicitly chosen.
    pub default_reasoning_effort: ReasoningEffort,
    /// Supported reasoning effort options.
    pub supported_reasoning_efforts: Vec<ReasoningEffortPreset>,
    /// Whether this model supports personality-specific instructions.
    #[serde(default)]
    pub supports_personality: bool,
    /// Deprecated: use `service_tiers` instead.
    #[serde(default)]
    pub additional_speed_tiers: Vec<String>,
    /// Service tiers this model can run with.
    #[serde(default)]
    pub service_tiers: Vec<ModelServiceTier>,
    /// Catalog default service tier id for this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_service_tier: Option<String>,
    /// Whether this is the default model for new users.
    pub is_default: bool,
    /// recommended upgrade model
    pub upgrade: Option<ModelUpgrade>,
    /// Whether this preset should appear in the picker UI.
    pub show_in_picker: bool,
    /// Multi-agent backend selected when this model starts a new thread.
    #[serde(default, skip_serializing, skip_deserializing)]
    #[schemars(skip)]
    #[ts(skip)]
    pub multi_agent_version: Option<MultiAgentVersion>,
    /// Availability NUX shown when this preset becomes accessible to the user.
    pub availability_nux: Option<ModelAvailabilityNux>,
    /// whether this model is supported in the api
    pub supported_in_api: bool,
    /// Input modalities accepted when composing user turns for this preset.
    #[serde(default = "default_input_modalities")]
    pub input_modalities: Vec<InputModality>,
}

/// Visibility of a model in the picker or APIs.
#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, TS, JsonSchema, EnumIter, Display,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ModelVisibility {
    List,
    Hide,
    None,
}

/// Shell execution capability for a model.
#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TS,
    JsonSchema,
    EnumIter,
    Display,
    Hash,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ConfigShellToolType {
    #[serde(alias = "default", alias = "local", alias = "shell_command")]
    UnifiedExec,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchToolType {
    Freeform,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS, JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchToolType {
    #[default]
    Text,
    TextAndImage,
}

/// Server-provided truncation policy metadata for a model.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TruncationMode {
    Bytes,
    Tokens,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolMode {
    Direct,
    CodeMode,
    CodeModeOnly,
}

fn deserialize_optional_model_selector<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    Ok(serde_json::from_value(serde_json::Value::String(value)).ok())
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, TS, JsonSchema)]
pub struct TruncationPolicyConfig {
    pub mode: TruncationMode,
    pub limit: i64,
}

impl TruncationPolicyConfig {
    pub const fn bytes(limit: i64) -> Self {
        Self {
            mode: TruncationMode::Bytes,
            limit,
        }
    }

    pub const fn tokens(limit: i64) -> Self {
        Self {
            mode: TruncationMode::Tokens,
            limit,
        }
    }
}

/// Semantic version triple encoded as an array in JSON (e.g. [0, 62, 0]).
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, TS, JsonSchema)]
pub struct ClientVersion(pub i32, pub i32, pub i32);

const fn default_effective_context_window_percent() -> i64 {
    95
}

const fn default_true() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_true(value: &bool) -> bool {
    *value
}

/// Model metadata returned by the Codex backend `/models` endpoint.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ModelInfo {
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_reasoning_level: Option<ReasoningEffort>,
    pub supported_reasoning_levels: Vec<ReasoningEffortPreset>,
    pub shell_type: ConfigShellToolType,
    pub visibility: ModelVisibility,
    pub supported_in_api: bool,
    pub priority: i32,
    #[serde(default)]
    pub additional_speed_tiers: Vec<String>,
    #[serde(default)]
    pub service_tiers: Vec<ModelServiceTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_service_tier: Option<String>,
    pub availability_nux: Option<ModelAvailabilityNux>,
    pub upgrade: Option<ModelInfoUpgrade>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_messages: Option<ModelMessages>,
    #[serde(default)]
    pub include_skills_usage_instructions: bool,
    #[serde(default)]
    pub include_plugin_usage_instructions: bool,
    #[serde(default = "default_true")]
    pub include_apps_usage_instructions: bool,
    /// Whether the model accepts the Responses API `reasoning.summary` parameter.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub supports_reasoning_summary_parameter: bool,
    #[serde(default)]
    pub default_reasoning_summary: ReasoningSummary,
    pub support_verbosity: bool,
    pub default_verbosity: Option<Verbosity>,
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    #[serde(default)]
    pub web_search_tool_type: WebSearchToolType,
    pub truncation_policy: TruncationPolicyConfig,
    #[serde(default)]
    pub supports_image_detail_original: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<i64>,
    /// Maximum context window allowed for config overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<i64>,
    /// Token threshold for automatic compaction. When omitted, core derives it
    /// from `context_window` (90%). When provided, core clamps it to 90% of the
    /// context window when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact_token_limit: Option<i64>,
    /// Opaque identifier for compaction-compatible model configurations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comp_hash: Option<String>,
    /// Percentage of the context window considered usable for inputs, after
    /// reserving headroom for system prompts, tool overhead, and model output.
    #[serde(default = "default_effective_context_window_percent")]
    pub effective_context_window_percent: i64,
    pub experimental_supported_tools: Vec<String>,
    /// Input modalities accepted by the backend for this model.
    #[serde(default = "default_input_modalities")]
    pub input_modalities: Vec<InputModality>,
    /// Internal-only marker set by core when a model slug resolved to fallback metadata.
    #[serde(default, skip_serializing, skip_deserializing)]
    #[schemars(skip)]
    #[ts(skip)]
    pub used_fallback_model_metadata: bool,
    #[serde(default)]
    pub supports_search_tool: bool,
    #[serde(default)]
    pub use_responses_lite: bool,
    #[serde(default)]
    pub node_repl_auto_review_required: bool,
    #[serde(default)]
    pub node_repl_disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review_model_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_specialty: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_model_selector"
    )]
    pub tool_mode: Option<ToolMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_model_selector"
    )]
    pub multi_agent_version: Option<MultiAgentVersion>,
    /// Reasoning effort used for multi-agent work when the user selects Ultra.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_agent_reasoning_effort: Option<ReasoningEffort>,
}

impl ModelInfo {
    pub fn resolved_context_window(&self) -> Option<i64> {
        self.context_window.or(self.max_context_window)
    }

    /// Context available to inference after reserving this model's configured headroom.
    pub fn usable_context_window(&self) -> Option<i64> {
        self.resolved_context_window().map(|context_window| {
            context_window.saturating_mul(self.effective_context_window_percent) / 100
        })
    }

    pub fn auto_compact_token_limit(&self) -> Option<i64> {
        let context_limit = self
            .resolved_context_window()
            .map(|context_window| (context_window * 9) / 10);
        let config_limit = self.auto_compact_token_limit;
        if let Some(context_limit) = context_limit {
            return Some(
                config_limit.map_or(context_limit, |limit| std::cmp::min(limit, context_limit)),
            );
        }
        config_limit
    }

    pub fn supports_personality(&self) -> bool {
        self.model_messages
            .as_ref()
            .is_some_and(ModelMessages::supports_personality)
    }

    pub fn get_model_instructions(&self, personality: Option<Personality>) -> String {
        if let Some(model_messages) = &self.model_messages
            && let Some(template) = &model_messages.instructions_template
        {
            if model_messages.instructions_variables.is_none() {
                return template.clone();
            }
            let personality_message = model_messages
                .get_personality_message(personality)
                .unwrap_or_default();
            template.replace(PERSONALITY_PLACEHOLDER, personality_message.as_str())
        } else {
            warn!(
                model = %self.slug,
                "Model has no instruction template; returning empty instructions."
            );
            String::new()
        }
    }
}

/// A strongly-typed template for assembling model instructions and developer messages.
///
/// When `instructions_variables` is absent, `instructions_template` is treated as literal text.
/// When variables are present but incomplete, missing values render as empty strings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ModelMessages {
    /// Additional developer instructions for persistent mode. Missing or null uses the built-in
    /// instructions; an empty string disables them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolMessages>,
    pub instructions_template: Option<String>,
    pub instructions_variables: Option<ModelInstructionsVariables>,
    pub approvals: Option<ApprovalMessages>,
    pub collaboration_modes: Option<CollaborationModeMessages>,
    pub auto_review: Option<AutoReviewMessages>,
    pub permissions: Option<PermissionMessages>,
    pub multi_agent: Option<MultiAgentMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<ModelTokenBudgetConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardian_v2: Option<GuardianV2ModelConfig>,
    /// Replacement confirmation-policy documents forwarded in actor MCP request metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation_policies: Option<ConfirmationPolicies>,
}

/// Model-owned confirmation-policy Markdown, forwarded unchanged to actor tools.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ConfirmationPolicies {
    /// Replacement Markdown for the Browser Use confirmation-policy document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_use: Option<String>,
    /// Replacement Markdown for the native Computer Use confirmation policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<String>,
}

/// Model-owned messages for built-in tools.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ToolMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_user_message_async: Option<ToolMessage>,
}

/// Model-owned messages for a built-in tool.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ToolMessage {
    /// Missing or null uses the built-in description; an empty string leaves the description
    /// empty without disabling the tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Model-owned defaults for the context-window token-budget feature.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ModelTokenBudgetConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub use_history_notes_extension: bool,
    pub reminder_threshold_tokens: i64,
    pub reminder_message_template: String,
    pub guidance_message: String,
    pub auto_compact_fallback_prompt: String,
    pub auto_compact_fallback_buffer_tokens: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ApprovalMessages {
    pub on_request: Option<String>,
    pub on_request_auto_review: Option<String>,
    pub never: Option<String>,
    pub unless_trusted: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct CollaborationModeMessages {
    pub default: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct AutoReviewMessages {
    pub policy: Option<String>,
    pub policy_template: Option<String>,
    /// Extra developer policy for `node_repl` and `cua_repl` reviews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_repl_policy: Option<String>,
    pub rejection_instructions: Option<String>,
    pub timeout_instructions: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct PermissionMessages {
    pub danger_full_access: Option<String>,
    pub workspace_write: Option<String>,
    pub read_only: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct MultiAgentMessages {
    pub role: Option<MultiAgentRoleMessages>,
    pub mode: Option<MultiAgentModeMessages>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct MultiAgentRoleMessages {
    pub root: Option<String>,
    pub subagent: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct MultiAgentModeMessages {
    pub explicit: Option<String>,
    /// Ultra-only mode instructions. Missing or null uses the built-in proactive hint;
    /// an empty string suppresses the mode message. `hint_text` takes precedence.
    pub proactive: Option<String>,
    pub hint_text: Option<String>,
}

impl ModelMessages {
    fn has_personality_placeholder(&self) -> bool {
        self.instructions_template
            .as_ref()
            .map(|spec| spec.contains(PERSONALITY_PLACEHOLDER))
            .unwrap_or(false)
    }

    fn supports_personality(&self) -> bool {
        self.has_personality_placeholder()
            && self
                .instructions_variables
                .as_ref()
                .is_some_and(ModelInstructionsVariables::is_complete)
    }

    pub fn get_personality_message(&self, personality: Option<Personality>) -> Option<String> {
        self.instructions_variables
            .as_ref()
            .and_then(|variables| variables.get_personality_message(personality))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ModelInstructionsVariables {
    pub personality_default: Option<String>,
    pub personality_friendly: Option<String>,
    pub personality_pragmatic: Option<String>,
}

impl ModelInstructionsVariables {
    pub fn is_complete(&self) -> bool {
        self.personality_default.is_some()
            && self.personality_friendly.is_some()
            && self.personality_pragmatic.is_some()
    }

    pub fn get_personality_message(&self, personality: Option<Personality>) -> Option<String> {
        if let Some(personality) = personality {
            match personality {
                Personality::None => Some(String::new()),
                Personality::Friendly => self.personality_friendly.clone(),
                Personality::Pragmatic => self.personality_pragmatic.clone(),
            }
        } else {
            self.personality_default.clone()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema)]
pub struct ModelInfoUpgrade {
    pub model: String,
    pub migration_markdown: String,
    /// Informational time when the model associated with this upgrade is scheduled to retire.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_rfc3339_timestamp"
    )]
    #[schemars(with = "Option<String>")]
    #[ts(type = "string", optional)]
    pub retirement_at: Option<DateTime<Utc>>,
}

fn deserialize_optional_rfc3339_timestamp<'de, D>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc)))
}

impl From<&ModelUpgrade> for ModelInfoUpgrade {
    fn from(upgrade: &ModelUpgrade) -> Self {
        ModelInfoUpgrade {
            model: upgrade.id.clone(),
            migration_markdown: upgrade.migration_markdown.clone().unwrap_or_default(),
            retirement_at: upgrade.retirement_at,
        }
    }
}

/// Response wrapper for `/models`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS, JsonSchema, Default)]
pub struct ModelsResponse {
    #[serde(
        serialize_with = "serialize_model_infos_with_legacy_base",
        deserialize_with = "deserialize_model_infos_with_legacy_base"
    )]
    pub models: Vec<ModelInfo>,
}

#[derive(Serialize)]
struct ModelInfoWithLegacyBaseInstructionsRef<'a> {
    #[serde(flatten)]
    model: &'a ModelInfo,
    base_instructions: String,
}

#[derive(Deserialize)]
struct ModelInfoWithLegacyBaseInstructions {
    #[serde(default)]
    base_instructions: Option<String>,
    #[serde(flatten)]
    model: ModelInfo,
}

/// Serializes catalog models with the deprecated top-level instruction field required by older
/// clients.
#[doc(hidden)]
pub fn serialize_model_infos_with_legacy_base<S>(
    models: &[ModelInfo],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    models
        .iter()
        .map(|model| ModelInfoWithLegacyBaseInstructionsRef {
            model,
            base_instructions: model.get_model_instructions(/*personality*/ None),
        })
        .collect::<Vec<_>>()
        .serialize(serializer)
}

/// Deserializes catalog models while promoting the legacy top-level instruction field into Model
/// Messages V2 when no canonical instruction template is present.
#[doc(hidden)]
pub fn deserialize_model_infos_with_legacy_base<'de, D>(
    deserializer: D,
) -> Result<Vec<ModelInfo>, D::Error>
where
    D: Deserializer<'de>,
{
    let models = Vec::<ModelInfoWithLegacyBaseInstructions>::deserialize(deserializer)?;
    models
        .into_iter()
        .map(|legacy_model| {
            let ModelInfoWithLegacyBaseInstructions {
                base_instructions,
                mut model,
            } = legacy_model;
            if let Some(base_instructions) = base_instructions
                && model
                    .model_messages
                    .as_ref()
                    .and_then(|messages| messages.instructions_template.as_ref())
                    .is_none()
            {
                let messages = model.model_messages.get_or_insert(ModelMessages {
                    persistent_instructions: None,
                    tools: None,
                    instructions_template: None,
                    instructions_variables: None,
                    approvals: None,
                    collaboration_modes: None,
                    auto_review: None,
                    permissions: None,
                    multi_agent: None,
                    token_budget: None,
                    confirmation_policies: None,
                    guardian_v2: None,
                });
                messages.instructions_template = Some(base_instructions);
            }
            if model
                .model_messages
                .as_ref()
                .and_then(|messages| messages.instructions_template.as_ref())
                .is_none()
            {
                let model_slug = &model.slug;
                return Err(D::Error::custom(format!(
                    "model `{model_slug}` is missing both `base_instructions` and \
                     `model_messages.instructions_template`"
                )));
            }
            Ok(model)
        })
        .collect()
}

// convert ModelInfo to ModelPreset
impl From<ModelInfo> for ModelPreset {
    fn from(info: ModelInfo) -> Self {
        let supports_personality = info.supports_personality();
        ModelPreset {
            id: info.slug.clone(),
            model: info.slug.clone(),
            display_name: info.display_name,
            description: info.description.unwrap_or_default(),
            model_specialty: info.model_specialty,
            default_reasoning_effort: info
                .default_reasoning_level
                .unwrap_or(ReasoningEffort::None),
            supported_reasoning_efforts: info.supported_reasoning_levels.clone(),
            supports_personality,
            additional_speed_tiers: info.additional_speed_tiers,
            service_tiers: info.service_tiers,
            default_service_tier: info.default_service_tier,
            is_default: false, // default is the highest priority available model
            upgrade: info.upgrade.as_ref().map(|upgrade| ModelUpgrade {
                id: upgrade.model.clone(),
                migration_config_key: info.slug.clone(),
                // todo(aibrahim): add the model link here.
                model_link: None,
                upgrade_copy: None,
                migration_markdown: Some(upgrade.migration_markdown.clone()),
                retirement_at: upgrade.retirement_at,
            }),
            show_in_picker: info.visibility == ModelVisibility::List,
            multi_agent_version: info.multi_agent_version,
            availability_nux: info.availability_nux,
            supported_in_api: info.supported_in_api,
            input_modalities: info.input_modalities,
        }
    }
}

impl ModelPreset {
    pub fn supports_fast_mode(&self) -> bool {
        self.service_tiers
            .iter()
            .any(|tier| tier.id == ServiceTier::Fast.request_value())
            || self
                .additional_speed_tiers
                .iter()
                .any(|tier| tier == SPEED_TIER_FAST)
    }
}

impl ModelInfo {
    pub fn supports_service_tier(&self, service_tier: &str) -> bool {
        self.service_tiers
            .iter()
            .any(|tier| tier.id == service_tier)
    }

    pub fn service_tier_for_request(&self, service_tier: Option<String>) -> Option<String> {
        service_tier.filter(|service_tier| {
            service_tier != SERVICE_TIER_DEFAULT_REQUEST_VALUE
                && self.supports_service_tier(service_tier)
        })
    }
}

impl ModelPreset {
    /// Filter models based on authentication mode.
    ///
    /// In ChatGPT mode, all models are visible. Otherwise, only API-supported models are shown.
    pub fn filter_by_auth(models: Vec<ModelPreset>, chatgpt_mode: bool) -> Vec<ModelPreset> {
        models
            .into_iter()
            .filter(|model| chatgpt_mode || model.supported_in_api)
            .collect()
    }

    /// Recompute the single default preset using picker visibility.
    ///
    /// The first picker-visible model wins; if none are picker-visible, the first model wins.
    pub fn mark_default_by_picker_visibility(models: &mut [ModelPreset]) {
        for preset in models.iter_mut() {
            preset.is_default = false;
        }
        if let Some(default) = models.iter_mut().find(|preset| preset.show_in_picker) {
            default.is_default = true;
        } else if let Some(default) = models.first_mut() {
            default.is_default = true;
        }
    }
}
