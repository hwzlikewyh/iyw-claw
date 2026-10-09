// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether a remembered answer still stands: one that both permissions are
/// granted does until a driver call says otherwise; one that something is
/// missing, for [`RECHECK_MISSING`].
#[cfg(any(test, target_os = "macos"))]
pub(super) fn answer_stands(report: &PermissionReport, age: Duration) -> bool {
    (report.accessibility && report.screen_recording) || age < RECHECK_MISSING
}
