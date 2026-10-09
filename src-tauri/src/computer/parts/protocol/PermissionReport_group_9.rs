// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The helper's OS permissions, as the helper itself sees them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReport {
    /// Whether this platform has per-application permissions at all. `false`
    /// on Windows and X11, where both flags below are reported `true`.
    pub required: bool,
    pub accessibility: bool,
    pub screen_recording: bool,
}
