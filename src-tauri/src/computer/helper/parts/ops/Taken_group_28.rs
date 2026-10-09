// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What a capture brought back, before it is scaled.
pub(super) struct Taken {
    pub(in crate::computer::helper::ops) png_base64: String,
    pub(in crate::computer::helper::ops) bounds: Rect,
    /// The backing scale the driver said the capture was taken at; `None`
    /// where it said nothing of it.
    pub(in crate::computer::helper::ops) scale: Option<f64>,
    pub(in crate::computer::helper::ops) title: Option<String>,
    /// Whether `bounds` are the window's as it was captured: read with the
    /// capture, or the same before it and after it.
    pub(in crate::computer::helper::ops) steady: bool,
}
