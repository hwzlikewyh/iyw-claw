// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// One running application, as the driver reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawApp {
    pub pid: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    /// The application's path on disk (its `.app` bundle on macOS, its
    /// executable elsewhere), when the platform says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub active: bool,
    /// An opaque, platform-specific stamp of when the process started. Only
    /// ever compared for equality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
}
