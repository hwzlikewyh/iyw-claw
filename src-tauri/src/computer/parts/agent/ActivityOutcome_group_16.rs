// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How an attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityOutcome {
    Done,
    /// No grant covered the window. The agent was told to ask.
    Refused,
    /// The grant was there and the read did not work — a missing OS
    /// permission, a window that closed mid-read. Reported so that "nothing
    /// on the strip" keeps meaning "nothing reached this window" rather than
    /// "nothing worked".
    Failed,
}

/// `computer://agent-activity`: one agent's one attempt on one window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerActivityPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actor: Option<crate::computer::tool_dispatch::ComputerActor>,
    pub target_id: String,
    pub action: ComputerAction,
    pub outcome: ActivityOutcome,
    /// Unix milliseconds.
    pub at: i64,
    /// The application, for what is done to one rather than to a window —
    /// starting it — where `target_id` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

pub const AGENT_ACTIVITY_EVENT: &str = "computer://agent-activity";
