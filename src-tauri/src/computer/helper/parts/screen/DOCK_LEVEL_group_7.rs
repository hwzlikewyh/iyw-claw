// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS: the window level the Dock keeps itself at.
#[cfg(target_os = "macos")]
pub(super) const DOCK_LEVEL: i64 = 20;

/// macOS: the applications whose every window shows what other
/// applications' windows hold — Stage Manager's strip of them, and the
/// banners of every application's notifications.
#[cfg(target_os = "macos")]
pub(super) const SHOWING_OTHERS: &[&str] =
    &["com.apple.WindowManager", "com.apple.notificationcenterui"];

/// macOS: whether a window of the application `bundle_id`, at `layer`,
/// shows what other applications' windows hold: one of [`SHOWING_OTHERS`],
/// or the Dock's away from its own level — where it draws every window at
/// once (Mission Control, an application's windows). The Dock at its level
/// is the Dock.
#[cfg(target_os = "macos")]
pub(super) fn shows_others(bundle_id: Option<&str>, layer: i64) -> bool {
    match bundle_id {
        Some("com.apple.dock") => layer != DOCK_LEVEL,
        Some(id) => SHOWING_OTHERS.contains(&id),
        None => false,
    }
}
