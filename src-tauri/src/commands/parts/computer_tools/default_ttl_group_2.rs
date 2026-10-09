// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn default_ttl() -> u32 {
    DEFAULT_GRANT_TTL_MINUTES
}

pub(super) fn default_stop_shortcut() -> String {
    StopShortcut::default_for(Platform::current()).to_string()
}

pub(super) fn default_show_indicator() -> bool {
    true
}

pub(super) fn default_allow_foreground() -> bool {
    true
}
