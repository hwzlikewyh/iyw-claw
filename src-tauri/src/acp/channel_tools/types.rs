use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CHANNEL_TOOL_NAMES: &[&str] = &[
    "list_message_channels",
    "save_message_channel",
    "delete_message_channel",
    "manage_channel_credential",
    "operate_message_channel",
    "list_channel_targets",
    "list_channel_messages",
    "send_channel_messages",
    "manage_channel_settings",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelToolRequest {
    pub token: String,
    pub tool: String,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct ChannelCaller {
    pub agent_type: String,
    pub session_ref: String,
    pub caller_scope: String,
    pub working_dir: std::path::PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListChannelsInput {
    pub channel_id: Option<i32>,
    pub channel_type: Option<String>,
    pub enabled: Option<bool>,
    pub runtime_status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveChannelInput {
    pub request_id: String,
    pub channel_id: Option<i32>,
    pub name: Option<String>,
    pub channel_type: Option<String>,
    pub enabled: Option<bool>,
    pub daily_report_enabled: Option<bool>,
    pub daily_report_time: Option<String>,
    pub config: Option<ChannelConfigInput>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelConfigInput {
    pub base_url: Option<String>,
    pub app_id: Option<String>,
    pub bot_id: Option<String>,
    pub client_id: Option<String>,
    pub default_target: Option<String>,
    pub default_target_type: Option<u8>,
    pub default_agent_type: Option<String>,
    pub poll_interval_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteChannelInput {
    pub request_id: String,
    pub channel_id: i32,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialOperation {
    Status,
    Set,
    Replace,
    Delete,
    StartAuthorization,
    CheckAuthorization,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialInput {
    pub request_id: Option<String>,
    pub channel_id: i32,
    pub operation: CredentialOperation,
    pub credential: Option<String>,
    pub authorization_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelOperation {
    Connect,
    Disconnect,
    QuickCheck,
    FullLoop,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperateChannelInput {
    pub request_id: String,
    pub channel_id: i32,
    pub operation: ChannelOperation,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListTargetsInput {
    pub channel_id: i32,
    pub target_kind: Option<String>,
    pub since: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListMessagesInput {
    pub channel_id: i32,
    pub target_id: Option<String>,
    pub direction: Option<String>,
    pub status: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<u64>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SendMessagesInput {
    pub request_id: String,
    pub items: Vec<SendItemInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SendItemInput {
    pub channel_id: i32,
    pub target_id: Option<String>,
    pub text: Option<String>,
    pub rich: Option<RichMessageInput>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RichMessageInput {
    pub title: Option<String>,
    pub body: String,
    #[serde(default)]
    pub fields: Vec<RichFieldInput>,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RichFieldInput {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsOperation {
    Get,
    Patch,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsInput {
    pub request_id: Option<String>,
    pub operation: SettingsOperation,
    pub patch: Option<SettingsPatchInput>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsPatchInput {
    pub command_prefix: Option<String>,
    pub message_language: Option<String>,
    pub event_filter: Option<Vec<String>>,
    pub reset_event_filter: Option<bool>,
    pub webhooks: Option<Vec<WebhookInput>>,
    pub natural_router: Option<NaturalRouterInput>,
    pub daily_report_enabled: Option<bool>,
    pub daily_report_time: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookInput {
    pub url: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRouterInput {
    pub enabled: bool,
    pub model: String,
    pub api_key: Option<String>,
    pub delete_api_key: Option<bool>,
}
