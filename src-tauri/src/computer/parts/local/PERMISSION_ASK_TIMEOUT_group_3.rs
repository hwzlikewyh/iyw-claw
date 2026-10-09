// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How long a helper started for one permission request has to answer. It
/// asks, then watches a couple of seconds for the system's dialog.
#[cfg(target_os = "macos")]
pub(super) const PERMISSION_ASK_TIMEOUT: Duration = Duration::from_secs(10);

/// The most a permission-request helper may print: one short line.
#[cfg(target_os = "macos")]
pub(super) const MAX_ASK_OUTPUT: u64 = 256;
