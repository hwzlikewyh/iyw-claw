// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What the system says of windows, for one listing. COM is entered while it
/// lives: the virtual desktops are asked through it.
pub struct Desktop {
    /// Released before COM is left: fields are dropped in order.
    pub(in crate::computer::helper::hwnd) desktops: Option<Object>,
    pub(in crate::computer::helper::hwnd) _com: Com,
}
