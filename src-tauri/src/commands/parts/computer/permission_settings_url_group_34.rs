// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The System Settings pane for one permission, as a URL; `None` off macOS,
/// where there is no such pane.
pub fn permission_settings_url(permission: OsPermission) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let pane = match permission {
        OsPermission::Accessibility => "Privacy_Accessibility",
        OsPermission::ScreenRecording => "Privacy_ScreenCapture",
    };
    Some(format!(
        "x-apple.systempreferences:com.apple.preference.security?{pane}"
    ))
}
