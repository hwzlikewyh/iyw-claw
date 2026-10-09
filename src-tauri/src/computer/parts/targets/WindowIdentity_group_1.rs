// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Which window, exactly. See the module note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowIdentity {
    pub pid: u32,
    /// `None` when the platform would not say; such a window is still
    /// listed, and matched on the other two fields alone.
    pub started_at: Option<u64>,
    pub window_id: u64,
    /// The run of the process drawing inside the window, where that is not
    /// its owner (`RawWindow::content`). See the module note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ProcessRun>,
}
