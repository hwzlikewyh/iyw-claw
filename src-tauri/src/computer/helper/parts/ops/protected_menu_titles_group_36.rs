// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The titles of the Apple menu and the application menu of `pid` (macOS):
/// what an application shared as a whole still does not reach.
#[cfg(target_os = "macos")]
pub(super) async fn protected_menu_titles(pid: u32) -> Option<[String; 2]> {
    crate::computer::helper::axwin::protected_menu_titles(pid).await
}
