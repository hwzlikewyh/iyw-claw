// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether a window listed with `before` and then `after` kept its size —
/// within a pixel, as a point's own check allows (`act::check_points`).
#[cfg(any(not(target_os = "linux"), test))]
pub(super) fn same_size(before: Option<&Rect>, after: Option<&Rect>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            (before.width - after.width).abs() <= 1.0 && (before.height - after.height).abs() <= 1.0
        }
        _ => false,
    }
}
