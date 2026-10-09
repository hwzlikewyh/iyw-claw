// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether a point on the screen, in desktop units, is in one of its
/// corners ([`CORNER`]). macOS only: elsewhere a corner does nothing of the
/// kind.
pub(super) fn in_a_corner(x: f64, y: f64) -> bool {
    #[cfg(target_os = "macos")]
    {
        let Some(screen) = main_display() else {
            // Where the screen is cannot be told: no corner can be ruled
            // out.
            return true;
        };
        let near = |v: f64, edge: f64| (v - edge).abs() < CORNER;
        let (left, top) = (screen.x, screen.y);
        let (right, bottom) = (screen.x + screen.width, screen.y + screen.height);
        (near(x, left) || near(x, right)) && (near(y, top) || near(y, bottom))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (x, y);
        false
    }
}
