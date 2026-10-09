// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How long starting an application may take before the driver answers.
pub(super) const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
