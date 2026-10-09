// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// An installed application as the driver lists it: who it is — its bundle
/// identifier, or its executable — for iyw-claw to judge by, and the command
/// that starts it, which is not who it is: a launch command carries
/// arguments, and on Linux may be a wrapper (`flatpak run …`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub app: RawApp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_path: Option<String>,
}

/// An application started for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawLaunch {
    /// The process, when the driver said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub name: String,
}

/// One run of a process: its pid, and the start stamp that tells it from a
/// later process under the same pid (`procinfo::process_start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRun {
    pub pid: u32,
    pub started_at: u64,
}

/// One normal window, as the driver reports it, joined with its application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawWindow {
    pub window_id: u64,
    pub pid: u32,
    /// Empty when the platform withholds it — on macOS, whenever the helper
    /// lacks Screen Recording.
    #[serde(default)]
    pub title: String,
    pub bounds: Rect,
    pub on_screen: bool,
    /// `None` when the platform cannot say. On macOS the driver never does;
    /// the helper asks Accessibility about the windows that could be, and on
    /// X11 the window manager (`helper::ops::mark_out_of_sight`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimized: Option<bool>,
    /// macOS: its application is hidden (⌘H), so the window is off the screen
    /// with nothing of its own having changed. `None` when that is not so or
    /// cannot be told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// `false` for a window on another Space (desktop); `None` when the
    /// platform cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_current_space: Option<bool>,
    /// Higher is closer to the front; `None` when the platform cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i64>,
    /// Windows: the process drawing what is inside the window, where that is
    /// not the process owning it — a packaged application, inside the frame
    /// `ApplicationFrameHost` draws for it (see `appident`). `app` is then
    /// that process's application, and the window is that application's only
    /// as long as this same run of it is inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ProcessRun>,
    pub app: RawApp,
}

/// A window screenshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCapture {
    /// PNG, base64.
    pub png_base64: String,
    /// The image's size, after the helper shrank it to the size asked for.
    pub width: u32,
    pub height: u32,
    /// The capture's size before it was shrunk: the window's own pixels. A
    /// point read off the image is scaled by `native / delivered` to reach
    /// the pixel the driver will act on.
    pub native_width: u32,
    pub native_height: u32,
    /// Whether `native_*` are known to be the window's pixels at full size —
    /// the driver runs with no ceiling on a capture, and the capture is the
    /// size of the window. Pointing by coordinates needs it.
    #[serde(default)]
    pub full_size: bool,
    pub window_bounds: Rect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A window's accessibility tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSnapshot {
    pub tree: String,
    pub element_count: u64,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_bounds: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The driver's id for this snapshot, which its elements are addressed
    /// by. `None` when the driver kept none (it could not match the window's
    /// accessibility surface): nothing in this tree can be acted on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// Every element that can be acted on, in tree order.
    #[serde(default)]
    pub refs: Vec<SnapshotRef>,
}

/// One element of a snapshot that can be acted on: the `[N]` in its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRef {
    pub index: u32,
    /// Where the element's line starts in `tree`, in bytes — so whoever cuts
    /// the tree short knows which refs the reader was shown.
    pub offset: u32,
    /// A password or other secret field: its value was taken out of the
    /// tree, and nothing is typed into it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

/// The driver's verdict on a set of predicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawVerify {
    pub status: VerifyStatus,
    pub stable: bool,
    pub samples: u64,
    pub elapsed_ms: u64,
    pub predicates: Vec<PredicateResult>,
}

/// Why the helper could not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperError {
    pub code: HelperErrorCode,
    pub message: String,
    /// Which permission is missing, for [`HelperErrorCode::PermissionMissing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<OsPermission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperErrorCode {
    /// The helper lacks an OS permission this op needs.
    PermissionMissing,
    /// No such window, or it no longer belongs to the process named.
    NoSuchWindow,
    /// The driver is not there, would not start, or died.
    DriverUnavailable,
    /// The driver file is not the pinned release: its signature, cdhash,
    /// runtime flag, entitlements or digest did not match. Not retried — the
    /// same file will fail the same way.
    DriverRejected,
    /// An op that needs the driver arrived before `Configure`.
    NotConfigured,
    /// The op itself was malformed.
    BadRequest,
    /// Anything else, in words.
    Failed,
    /// No input goes anywhere right now: the session is locked, or another
    /// user's is active.
    Paused,
    /// The person pressed Stop after this was sent, or before it was let
    /// through: it was cut off.
    Stopped,
    /// The element or point is from a snapshot or capture the window has
    /// moved past — a newer snapshot replaced it, the window changed size.
    StaleRef,
    /// The element or point is not in the window.
    OutOfTarget,
    /// The window cannot take input in the background right now: minimized,
    /// hidden, on another desktop, or its application has another window the
    /// keys could reach instead.
    Occluded,
    /// The application offers no route for this action in the background.
    BackgroundUnavailable,
    /// The element is a password or other secret field.
    SecretField,
    /// The action was allowed and did not happen: a disabled control, no such
    /// option, more text than one call can type.
    ActionFailed,
    /// A paste by another route — a menu command or a control named for
    /// pasting: it would write the person's clipboard into the window.
    PasteRefused,
    /// A menu that reaches past the application shared: the Apple menu and
    /// the application menu.
    BeyondApp,
}
