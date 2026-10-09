// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A shared window, for iyw-claw's own UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedWindow {
    pub target_id: String,
    pub app_name: String,
    pub app_key: String,
    pub title: String,
    pub level: GrantLevel,
    pub granted_at: i64,
    pub last_used_at: i64,
    /// Shared with its whole application ([`SharedApp`]), not on its own.
    #[serde(default)]
    pub whole_app: bool,
    /// The share of that application, when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// Shared with the entire screen ([`SharedScreen`]).
    #[serde(default)]
    pub whole_screen: bool,
}

/// Which application, exactly: the run of the process that owns its
/// windows, which application that is (a frame host is several), and the run
/// drawing inside its frames where another process does. An application
/// relaunched is another one, never shared.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppIdentity {
    pub pid: u32,
    pub started_at: u64,
    pub key: String,
    pub content: Option<ProcessRun>,
}
