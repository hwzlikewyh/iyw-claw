// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The scale of a capture `width` pixels wide of a window whose bounds are
/// `bounds`, as the driver reckons it. In points (`points`: macOS) a window
/// is captured at 1× or 2× — whichever its size is nearer; elsewhere bounds
/// are in the pixels captured.
pub(super) fn reckoned_scale(width: u32, bounds: &Rect, points: bool) -> f64 {
    if !points || bounds.width <= 0.0 {
        return 1.0;
    }
    let ratio = f64::from(width) / bounds.width;
    if (ratio - 1.0).abs() <= (ratio - 2.0).abs() {
        1.0
    } else {
        2.0
    }
}
