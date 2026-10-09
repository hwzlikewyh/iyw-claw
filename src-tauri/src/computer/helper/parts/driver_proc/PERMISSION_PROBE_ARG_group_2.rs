// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The argument that has the pinned driver answer one question and exit:
/// what TCC lets its responsible process — the helper — do. Checked by the
/// driver before anything else runs, logging and telemetry included.
#[cfg(target_os = "macos")]
pub(super) const PERMISSION_PROBE_ARG: &str = "--cua-internal-permission-probe";

/// How long a permission probe has to answer.
#[cfg(target_os = "macos")]
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The most a probe may print; its answer is one short line.
#[cfg(target_os = "macos")]
pub(super) const MAX_PROBE_OUTPUT: u64 = 4096;
