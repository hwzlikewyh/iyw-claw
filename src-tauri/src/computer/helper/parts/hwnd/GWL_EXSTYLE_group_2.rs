// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// `GWL_EXSTYLE`, and the two extended styles of a window every click
/// passes through: layered and transparent to the pointer.
pub(super) const GWL_EXSTYLE: i32 = -20;

pub(super) const WS_EX_TRANSPARENT: u32 = 0x0000_0020;

pub(super) const WS_EX_LAYERED: u32 = 0x0008_0000;

/// `DWMWA_EXTENDED_FRAME_BOUNDS`: a window's frame as the compositor draws
/// it, in physical pixels whatever this process's own scaling.
pub(super) const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

/// The most top-level windows one walk of the screen takes in.
pub(super) const MAX_SCREEN_WINDOWS: usize = 8192;

/// `SW_SHOWNOACTIVATE`: back to its most recent size and place, without
/// being made the active window.
pub(super) const SW_SHOWNOACTIVATE: i32 = 4;
