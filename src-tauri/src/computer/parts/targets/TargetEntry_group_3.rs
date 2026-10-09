// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// One window iyw-claw has named.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetEntry {
    pub target_id: String,
    pub identity: WindowIdentity,
    pub app: RawApp,
    /// The title as last seen. Handed to an agent only through
    /// [`visible_title`]; shown to the person in iyw-claw's own UI.
    pub title: String,
    pub bounds: Rect,
    pub on_screen: bool,
    pub minimized: Option<bool>,
    /// macOS: its application is hidden (⌘H).
    pub hidden: Option<bool>,
    pub on_current_space: Option<bool>,
    pub grant: Option<ComputerGrant>,
    /// Moves on every transition into or out of a grant, so a generation
    /// minted under one grant never names a read made under another.
    pub epoch: u64,
    /// Reads completed under the current epoch.
    pub reads: u64,
    /// The latest snapshot read under the current grant: what a ref is
    /// resolved against.
    pub snapshot_mark: Option<SnapshotMark>,
    /// The latest screenshot read under the current grant: what a point is
    /// read in.
    pub capture_mark: Option<CaptureMark>,
    /// The window stopped turning up while it was shared. Kept, grant-less,
    /// so a later call on its id is told "not shared" — the same answer as a
    /// window nobody shared, as the browser answers for a tab that navigated
    /// away — rather than "no such window", which would make an id that was
    /// once valid look like one the agent invented.
    pub gone: bool,
}
