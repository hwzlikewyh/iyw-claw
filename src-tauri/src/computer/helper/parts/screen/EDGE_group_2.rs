// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How far past a window that is never shared the picture is painted over
/// and a point refused, in desktop units: the pointer takes a window by its
/// edge a little outside what it draws (to resize it) — on Windows by an
/// invisible border the frame the compositor reports leaves out, as wide as
/// the display is scaled.
#[cfg(windows)]
pub(super) const EDGE: f64 = 16.0;
