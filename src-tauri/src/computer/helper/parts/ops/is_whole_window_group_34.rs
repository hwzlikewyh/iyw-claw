// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether a capture of `width` × `height` pixels is the whole window at its
/// own resolution: the window's bounds times the backing scale, give or take
/// the frame the platforms crop differently (a few pixels, or a few percent).
/// A capture the driver had shrunk would be far smaller.
pub(super) fn is_whole_window(width: u32, height: u32, bounds: &Rect, scale: f64) -> bool {
    if bounds.is_empty() {
        return false;
    }
    let near = |got: u32, expected: f64| {
        let got = f64::from(got);
        let slack = (expected * 0.05).max(16.0);
        (got - expected).abs() <= slack
    };
    near(width, bounds.width * scale) && near(height, bounds.height * scale)
}
