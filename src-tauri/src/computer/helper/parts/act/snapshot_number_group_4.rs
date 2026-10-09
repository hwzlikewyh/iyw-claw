// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The number in a driver snapshot id (`s` and eight hex digits).
pub(super) fn snapshot_number(id: &str) -> Option<u64> {
    u64::from_str_radix(id.strip_prefix('s')?, 16).ok()
}

pub(super) fn is_safari(app_key: &str) -> bool {
    let key = app_key.to_ascii_lowercase();
    key.starts_with("com.apple.safari") || key.ends_with("/safari.app")
}
