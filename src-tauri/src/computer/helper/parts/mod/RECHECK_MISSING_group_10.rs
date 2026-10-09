// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How long an answer that a permission is missing stands before the helper
/// asks the system again. Asking starts a process; an agent retrying a
/// screenshot in a loop should not start one per try.
#[cfg(any(test, target_os = "macos"))]
pub(super) const RECHECK_MISSING: Duration = Duration::from_secs(2);
