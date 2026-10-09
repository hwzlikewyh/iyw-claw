// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A rectangle in the platform's desktop coordinate space: points on macOS,
/// physical pixels on Windows, X11 pixels on Linux — whatever the platform
/// reports window bounds in. Never mixed with screenshot pixels: a capture
/// carries its own `width` / `height` and the window bounds it was taken of,
/// so the two spaces stay separate values rather than one number read two
/// ways.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
