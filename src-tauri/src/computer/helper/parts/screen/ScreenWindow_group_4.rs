// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// One window on the screen, in desktop units, and whether the rules let it
/// be seen and touched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenWindow {
    /// The system's own number for it, the same from one listing to the next.
    pub id: u64,
    pub bounds: Rect,
    pub allowed: bool,
    /// A click on it reaches it — false for an overlay every click passes
    /// through, which only hides what is under it.
    pub takes_clicks: bool,
}
