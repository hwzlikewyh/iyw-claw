// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Remembers which application each running process is, so listing windows
/// does not re-list every installed application each time. Keyed by pid AND
/// start time: a reused pid is a cache miss, never a stale hit. Not used on
/// macOS or Windows, where each listing reads the owners afresh (see
/// [`list_windows`]).
#[derive(Default)]
pub struct AppCache {
    #[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
    pub(in crate::computer::helper::ops) apps: HashMap<(u32, Option<u64>), RawApp>,
}
