// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// iyw-claw's own bundle identifier (`tauri.conf.json`), matched on any process
/// so a second iyw-claw — a development build next to the installed one — is as
/// out of reach as this one.
pub const IYW_CLAW_BUNDLE_ID: &str = "app.iywclaw";

/// The file names iyw-claw's executable goes by, matched on any process's path
/// for the same reason, where an application has no bundle identifier to
/// know it by: on Windows, a development build next to the installed one is
/// otherwise just another executable.
pub const IYW_CLAW_EXECUTABLES: &[&str] = &[
    "iyw-claw.exe",
    "iyw-claw",
    "iyw-claw-server.exe",
    "iyw-claw-server",
    "iyw-computer-helper.exe",
    "iyw-computer-helper",
];

/// A grant in force on one window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerGrant {
    /// Never [`GrantLevel::None`]: a window with no grant carries no
    /// `ComputerGrant` at all, so "level none, but still bound to a window" is
    /// a state that cannot be written down.
    pub level: GrantLevel,
    /// Unix milliseconds.
    pub granted_at: i64,
    /// Unix milliseconds of the last read this grant allowed.
    pub last_used_at: i64,
    /// Whether the window was shared on its own, or with its whole
    /// application.
    #[serde(default)]
    pub scope: GrantScope,
}

/// What a person shared when they shared a window: the window, or its whole
/// application — every window of it, the ones it opens later included, its
/// menus and its own shortcuts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantScope {
    #[default]
    Window,
    App,
    /// Shared with the entire screen: every window the rules allow, and the
    /// desktop's own shortcuts too — but never the keys that lock the screen
    /// or log out.
    Screen,
}
