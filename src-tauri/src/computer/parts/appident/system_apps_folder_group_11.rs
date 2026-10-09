// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(not(windows))]
pub(super) fn system_apps_folder() -> Option<&'static str> {
    None
}
