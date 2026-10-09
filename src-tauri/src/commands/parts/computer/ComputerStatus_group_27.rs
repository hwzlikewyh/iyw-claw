// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

// -------- The person's side: iyw-claw's Computer use panel ----------------------

/// Everything the panel shows at a glance.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerStatus {
    pub enabled: bool,
    /// `macos` / `windows` / `linux`.
    pub platform: &'static str,
    /// Whether this platform's driver has passed iyw-claw's release matrix.
    /// `false` everywhere until it has; the panel says "preview".
    pub verified_platform: bool,
    pub backend: BackendStatus,
    /// The helper's own permissions, when the helper could be asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionReport>,
    /// iyw-claw's own TCC standing (macOS only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<HostTccStatus>,
    pub shared: Vec<SharedWindow>,
    /// Whether the share picker offers the entire screen: macOS and
    /// Windows, with its switch on.
    pub screen_offered: bool,
}

/// One window, as the share picker shows it to the person — title and all:
/// it is their own screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerWindow {
    pub target_id: String,
    pub app_name: String,
    pub app_key: String,
    pub pid: u32,
    pub title: String,
    pub bounds: Rect,
    pub on_screen: bool,
    pub minimized: bool,
    /// Its application is hidden (macOS ⌘H).
    pub hidden: bool,
    pub level: GrantLevel,
    /// Shared with its whole application, not on its own.
    pub whole_app: bool,
    /// That application's share, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// Shared with the entire screen.
    pub whole_screen: bool,
    /// Why it can never be shared, when that is so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_grantable: Option<NotGrantable>,
}

/// `macos` / `windows` / `linux`: the machine whose screen this is.
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}
