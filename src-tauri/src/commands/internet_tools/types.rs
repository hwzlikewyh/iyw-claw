use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub(super) const AGENT_REACH_VERSION: &str = "1.5.0";
pub(super) const OPENCLI_VERSION: &str = "1.8.6";
pub(super) const MCPORTER_VERSION: &str = "0.9.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternetToolId {
    AgentReach,
    Opencli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternetToolStatus {
    Installed,
    UpdateAvailable,
    NotRunnable,
    NotInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternetChannelHealth {
    Ok,
    Warn,
    Error,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternetChannelStatus {
    pub id: String,
    pub status: InternetChannelHealth,
    pub name: String,
    pub message: String,
    pub tier: u8,
    pub backends: Vec<String>,
    pub active_backend: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentReachConfigKey {
    Proxy,
    GithubToken,
    GroqKey,
    OpenaiKey,
    TwitterCookies,
    YoutubeCookies,
    XhsCookies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportedBrowser {
    Chrome,
    Edge,
    Firefox,
    Brave,
    Opera,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentReachChannel {
    Twitter,
    Xiaoyuzhou,
    Xueqiu,
    Xiaohongshu,
    Reddit,
    Bilibili,
    Linkedin,
}

impl AgentReachConfigKey {
    pub(super) fn cli_value(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::GithubToken => "github-token",
            Self::GroqKey => "groq-key",
            Self::OpenaiKey => "openai-key",
            Self::TwitterCookies => "twitter-cookies",
            Self::YoutubeCookies => "youtube-cookies",
            Self::XhsCookies => "xhs-cookies",
        }
    }
}

impl SupportedBrowser {
    pub(super) fn cli_value(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
            Self::Brave => "brave",
            Self::Opera => "opera",
        }
    }
}

impl AgentReachChannel {
    pub(super) fn cli_value(self) -> &'static str {
        match self {
            Self::Twitter => "twitter",
            Self::Xiaoyuzhou => "xiaoyuzhou",
            Self::Xueqiu => "xueqiu",
            Self::Xiaohongshu => "xiaohongshu",
            Self::Reddit => "reddit",
            Self::Bilibili => "bilibili",
            Self::Linkedin => "linkedin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InternetToolInfo {
    pub id: InternetToolId,
    pub status: InternetToolStatus,
    pub installed: bool,
    pub version: Option<String>,
    pub expected_version: String,
    pub path: Option<String>,
    pub runtime_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InternetToolSkill {
    pub id: String,
    pub source: InternetToolId,
    pub installed_centrally: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InternetSkillSyncReport {
    pub synced: usize,
    pub skill_ids: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpencliDoctorResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct RawInternetChannelStatus {
    status: InternetChannelHealth,
    name: String,
    message: String,
    tier: u8,
    #[serde(default)]
    backends: Vec<String>,
    active_backend: Option<String>,
}

pub(super) fn parse_agent_reach_doctor_json(
    raw: &str,
) -> Result<Vec<InternetChannelStatus>, String> {
    let channels = serde_json::from_str::<BTreeMap<String, RawInternetChannelStatus>>(raw)
        .map_err(|error| format!("invalid Agent Reach doctor output: {error}"))?;
    Ok(channels
        .into_iter()
        .map(|(id, channel)| InternetChannelStatus {
            id,
            status: channel.status,
            name: channel.name,
            message: channel.message,
            tier: channel.tier,
            backends: channel.backends,
            active_backend: channel.active_backend,
        })
        .collect())
}

pub(super) fn tool_status(
    installed: bool,
    version: Option<&str>,
    expected_version: &str,
    runtime_error: Option<&str>,
) -> InternetToolStatus {
    if !installed {
        return InternetToolStatus::NotInstalled;
    }
    if runtime_error.is_some() || version.is_none() {
        return InternetToolStatus::NotRunnable;
    }
    if version != Some(expected_version) {
        return InternetToolStatus::UpdateAvailable;
    }
    InternetToolStatus::Installed
}

pub(super) fn expected_version(tool: InternetToolId) -> &'static str {
    match tool {
        InternetToolId::AgentReach => AGENT_REACH_VERSION,
        InternetToolId::Opencli => OPENCLI_VERSION,
    }
}
