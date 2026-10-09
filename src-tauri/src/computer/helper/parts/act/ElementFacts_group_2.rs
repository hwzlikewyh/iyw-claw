// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What the tree said of one element that can be acted on.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementFacts {
    pub role: String,
    pub secret: bool,
    /// A menu command or a button named for pasting (`ops::element_refs`).
    pub paste: bool,
    /// Where the element was on the screen when the snapshot was taken, in
    /// the platform's desktop units — for the marker, not for aiming (the
    /// driver aims by the element itself).
    pub frame: Option<Rect>,
}

/// The latest snapshot of one window: the driver's id for it, and its
/// elements.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotFacts {
    pub snapshot_id: String,
    pub elements: HashMap<u32, ElementFacts>,
}

/// The latest snapshot the helper took of each window, as the driver keeps
/// them: a new snapshot of a window replaces the one before.
#[derive(Default)]
pub struct SnapshotBook {
    pub(in crate::computer::helper::act) windows: HashMap<(u32, u64), SnapshotFacts>,
    /// Oldest first, for letting go once there are too many.
    pub(in crate::computer::helper::act) order: VecDeque<(u32, u64)>,
}
