// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Permission for one action, with the action as the helper is to carry it
/// out: every ref and point resolved against what the agent last read.
#[derive(Debug, Clone, PartialEq)]
pub struct ActTicket {
    pub target_id: String,
    pub identity: WindowIdentity,
    /// The sharing the action was let through under (see
    /// [`TargetEntry::epoch`]): a key pressed again and again is held to the
    /// sharing its first press went out under.
    pub epoch: u64,
    pub app: RawApp,
    pub action: WindowAction,
    pub aim: Aim,
}

/// Where on the screen an action lands, as far as iyw-claw can place it once
/// the helper says where it aimed. For the marker that shows the person
/// where an agent acted; nothing is decided by it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aim {
    /// Wherever the window's focus is: a key with no element, a scroll with
    /// no target.
    Focus,
    /// At the element, where its snapshot found it.
    Element,
    /// This far from the window's top-left corner, in desktop units.
    Offset { x: f64, y: f64 },
}
