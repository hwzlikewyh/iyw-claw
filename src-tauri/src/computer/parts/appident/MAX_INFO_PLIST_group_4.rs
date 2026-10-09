// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The largest `Info.plist` read: real ones are a few kilobytes, and the
/// helper does not read an unbounded file because a bundle says so.
#[cfg(target_os = "macos")]
pub(super) const MAX_INFO_PLIST: u64 = 1 << 20;
