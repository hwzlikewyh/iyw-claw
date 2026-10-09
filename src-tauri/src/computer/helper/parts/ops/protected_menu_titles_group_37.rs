// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Elsewhere a window's menus are its own (see `without_app_menus`).
#[cfg(not(target_os = "macos"))]
pub(super) async fn protected_menu_titles(_pid: u32) -> Option<[String; 2]> {
    None
}
