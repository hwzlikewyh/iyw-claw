// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The longest the walk through an application's menus may take, however
/// many items they hold: each question is bounded on its own ([`TIMEOUT`]),
/// and this bounds them all.
pub(super) const MENU_WALK: std::time::Duration = std::time::Duration::from_secs(3);
