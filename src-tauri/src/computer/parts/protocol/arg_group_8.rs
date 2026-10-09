// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl OsPermission {
    /// How [`REQUEST_PERMISSION_ARG`] names it.
    pub fn arg(self) -> &'static str {
        match self {
            OsPermission::Accessibility => "accessibility",
            OsPermission::ScreenRecording => "screen-recording",
        }
    }

    pub fn from_arg(arg: &str) -> Option<Self> {
        [OsPermission::Accessibility, OsPermission::ScreenRecording]
            .into_iter()
            .find(|p| p.arg() == arg)
    }
}
