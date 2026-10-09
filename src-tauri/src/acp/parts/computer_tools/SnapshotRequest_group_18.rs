// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What `computer_snapshot` asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRequest {
    /// Cap on the returned tree in characters. `None` →
    /// [`DEFAULT_SNAPSHOT_MAX_CHARS`]; `Some(0)` → no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_elements: Option<u32>,
    /// Keep only the lines mentioning this (and their ancestors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// What `computer_snapshot` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerSnapshotOutcome {
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<WindowSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
