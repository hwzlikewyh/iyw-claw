// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS: how near a corner of the screen a point is refused, in desktop
/// points — the pointer arriving in a corner sets off what the person set it
/// to (Mission Control, locking the screen, sleeping the display).
#[cfg(target_os = "macos")]
pub(super) const CORNER: f64 = 8.0;
