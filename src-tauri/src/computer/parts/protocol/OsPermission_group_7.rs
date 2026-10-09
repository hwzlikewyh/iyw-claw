// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A permission the helper may need from the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OsPermission {
    /// macOS Accessibility: reading the element tree.
    Accessibility,
    /// macOS Screen Recording: screenshots, and other applications' window
    /// titles.
    ScreenRecording,
}
