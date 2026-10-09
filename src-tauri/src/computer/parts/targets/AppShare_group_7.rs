// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// An application shared as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct AppShare {
    /// iyw-claw's own name for the share, for the panel to change or end it by.
    pub app_id: String,
    /// Its clock is the application's: any window of it being used moves it.
    pub grant: ComputerGrant,
    pub app: RawApp,
}

/// An application shared as a whole, for iyw-claw's own UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedApp {
    pub app_id: String,
    pub app_name: String,
    pub app_key: String,
    pub level: GrantLevel,
    pub granted_at: i64,
    pub last_used_at: i64,
    /// How many of its windows are shared with it now.
    pub windows: u32,
}

/// The id the entire screen goes by among targets: what an agent reads and
/// points at while the person shares the screen as a whole.
pub const SCREEN_TARGET_ID: &str = "d1";

/// The entire screen, shared as a whole.
#[derive(Debug, Clone)]
pub struct ScreenShare {
    /// Its clock is the screen's: the screen, or any window shared with it,
    /// being used moves it.
    pub grant: ComputerGrant,
    /// See [`TargetEntry::epoch`]: a new one for every sharing of the
    /// screen, so a picture read under one never names a point under the
    /// next.
    pub epoch: u64,
    /// Pictures of the screen read under this sharing.
    pub reads: u64,
    /// The latest picture of the screen read under it: what a point on the
    /// screen is read in.
    pub capture_mark: Option<CaptureMark>,
    /// The rules as they stood when the sharing last followed them — shared,
    /// or swept: what a window a later listing finds is judged by before it
    /// takes the screen's grant.
    pub(in crate::computer::targets) me: SelfIdentity,
    pub(in crate::computer::targets) blocklist: Blocklist,
}

/// The entire screen shared, for iyw-claw's own UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedScreen {
    pub level: GrantLevel,
    pub granted_at: i64,
    pub last_used_at: i64,
    /// How many windows are shared with it now.
    pub windows: u32,
}

/// Permission for one read of the entire screen, taken before the read and
/// checked again after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenReadTicket {
    pub epoch: u64,
}

/// Permission for one action on the entire screen, with the action as the
/// helper is to carry it out: every point resolved against the latest
/// picture of the screen, as the agent was given it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenActTicket {
    /// The sharing the action was let through under.
    pub epoch: u64,
    pub action: WindowAction,
    /// The screen as the picture the points were read off was taken.
    pub geometry: ScreenGeometry,
}

/// The latest snapshot an agent read of a window: the generation that named
/// it, and which of its elements the agent may now act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMark {
    pub generation: String,
    /// The driver's id for the snapshot; `None` when it kept none, and
    /// nothing in the tree can be acted on.
    pub snapshot_id: Option<String>,
    /// The refs whose lines the agent was given.
    pub shown: BTreeSet<u32>,
    /// The refs the tree had and the agent's copy was cut short of
    /// (`maxChars`): told apart from refs that never were, so the refusal can
    /// say which.
    pub cut: BTreeSet<u32>,
    /// The refs of secret fields.
    pub secret: BTreeSet<u32>,
}

/// The latest screenshot an agent read of a window: the generation that
/// named it, and the geometry a point read off it is mapped back through.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureMark {
    pub generation: String,
    /// The image as the agent got it.
    pub width: u32,
    pub height: u32,
    /// The window's own pixels, which the image was shrunk from.
    pub native_width: u32,
    pub native_height: u32,
    /// Whether the native size is known to be the window's full size (see
    /// `RawCapture::full_size`). Points need it.
    pub full_size: bool,
    pub window_bounds: Rect,
}

/// What a read leaves behind for later actions, before it has a generation.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadMark {
    Snapshot {
        snapshot_id: Option<String>,
        shown: BTreeSet<u32>,
        cut: BTreeSet<u32>,
        secret: BTreeSet<u32>,
    },
    Capture {
        width: u32,
        height: u32,
        native_width: u32,
        native_height: u32,
        full_size: bool,
        window_bounds: Rect,
    },
}

/// Why a read may not go ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadRefusal {
    /// iyw-claw never named a window by this id.
    NoSuchTarget,
    /// The window is not shared — never was, no longer is, or has gone.
    GrantRequired,
    /// The window can never be shared.
    NotGrantable(NotGrantable),
}

/// Permission for one read, taken before the read and checked again after.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadTicket {
    pub target_id: String,
    pub identity: WindowIdentity,
    pub epoch: u64,
    pub app: RawApp,
    pub bounds: Rect,
    /// Shared on its own, or with its application — whose menus a read then
    /// takes in too.
    pub scope: GrantScope,
}

/// Why an action may not go ahead, before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActDenied {
    /// iyw-claw never named a window by this id.
    NoSuchTarget,
    /// Not shared — never was, no longer is, or gone.
    GrantRequired,
    /// Shared for reading only.
    ControlRequired,
    /// The window can never be shared.
    NotGrantable(NotGrantable),
    /// A ref or point that is not from the window's latest snapshot or
    /// screenshot, or not in it.
    Stale(Staleness),
    /// A point outside the image it was read off.
    OutOfImage,
    /// Text into a secret field.
    Secret,
    /// A key a window grant does not reach — it acts on the application or
    /// the desktop.
    ChordBeyond,
    /// A paste: the clipboard is the user's, and its source is not tracked.
    Paste,
    /// A character key with no element named to type it into.
    NeedsElement,
    /// The screenshot the point came from cannot be mapped back to the
    /// window's pixels.
    NoPointing,
    /// Keys held over a drag where the driver would drag without them.
    DragModifiers,
    /// Keys held over a double-click in a window, which the driver's
    /// double-click does not hold (macOS and Linux refuse them, Windows
    /// double-clicks without them).
    DoubleClickModifiers,
    /// A key that is the desktop's own, which not even a grant on the whole
    /// application reaches.
    DesktopChord,
    /// Something only an application shared as a whole allows — its menus.
    AppGrantRequired,
    /// A menu command on a system whose driver cannot choose one (Windows).
    MenusUnavailable,
    /// A frame no window can have: a side under the smallest, or a number
    /// out of range.
    BadFrame,
    /// A key that locks the screen or logs out, which no sharing reaches —
    /// not even the entire screen's.
    SessionChord,
    /// Asked of the entire screen, something other than a click, a drag or a
    /// scroll at a point of its picture: keys, typing and the rest go to a
    /// window.
    ScreenPointerOnly,
}

/// How a ref or point is out of date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// No snapshot has been read under the current sharing.
    NoSnapshot,
    /// The generation is not the latest snapshot's.
    OldSnapshot,
    /// The driver kept no snapshot of the window: nothing in the tree can be
    /// acted on.
    NotActionable,
    /// The ref was in the tree, past where the agent's copy was cut.
    CutAway(u32),
    /// The latest snapshot has no such ref.
    NoSuchRef(u32),
    /// No screenshot has been read under the current sharing.
    NoCapture,
    /// The generation is not the latest screenshot's.
    OldCapture,
}
