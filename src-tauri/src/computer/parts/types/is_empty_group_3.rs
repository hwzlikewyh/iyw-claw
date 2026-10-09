// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Rect {
    /// Whether this rectangle has an area. A window the platform reports at
    /// zero size is not one a person could have meant to share, and has
    /// nothing a capture could show.
    pub fn is_empty(&self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }
}
