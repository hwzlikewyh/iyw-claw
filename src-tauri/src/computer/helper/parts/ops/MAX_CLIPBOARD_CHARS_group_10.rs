// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The most clipboard text handed back to an agent at once.
pub(super) const MAX_CLIPBOARD_CHARS: usize = 100_000;
