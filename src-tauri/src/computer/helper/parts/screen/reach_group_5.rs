// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ScreenWindow {
    /// What of the screen is this window's to keep from sight and touch:
    /// its frame and the edge around it ([`EDGE`]).
    pub(in crate::computer::helper::screen) fn reach(&self) -> Rect {
        Rect {
            x: self.bounds.x - EDGE,
            y: self.bounds.y - EDGE,
            width: self.bounds.width + 2.0 * EDGE,
            height: self.bounds.height + 2.0 * EDGE,
        }
    }
}
