// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Why a share did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareError {
    NoSuchTarget,
    Gone,
    NotGrantable(NotGrantable),
    /// The window is shared with its whole application: what it is shared
    /// for is the application's, and changes with it.
    AppShared,
    /// The entire screen is shared: what any window or application is shared
    /// for is the screen's, and changes with it.
    ScreenShared,
}

/// What sharing an application or the entire screen, or ending its share,
/// changed: the windows whose share moved with it, and whether a share beyond
/// the windows' own — an application's, or the screen's — did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppChange {
    pub windows: Vec<ComputerGrantPayload>,
    pub app_changed: bool,
}
