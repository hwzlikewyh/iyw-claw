// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl PermissionReport {
    /// Whether `permission` is granted.
    pub fn has(&self, permission: OsPermission) -> bool {
        match permission {
            OsPermission::Accessibility => self.accessibility,
            OsPermission::ScreenRecording => self.screen_recording,
        }
    }
}
