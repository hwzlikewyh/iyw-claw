// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What one application says of its windows (macOS, see `crate::computer::helper::axwin`).
#[cfg(any(test, target_os = "macos"))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppWindows {
    /// Whether the application is hidden (⌘H); `None` when it would not say.
    pub hidden: Option<bool>,
    /// Each window it lists, by window id: whether it is minimized — `None`
    /// for one that would not say. Empty when its windows were not asked
    /// for.
    pub minimized: HashMap<u64, Option<bool>>,
}
