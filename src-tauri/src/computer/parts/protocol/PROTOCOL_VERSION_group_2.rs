// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Bumped whenever a frame changes shape. The helper ships in the same bundle
/// as iyw-claw, so a mismatch means a broken install (a helper left behind by a
/// partial update), and iyw-claw refuses to talk to it rather than guess.
pub const PROTOCOL_VERSION: u32 = 14;

/// A fingerprint of the sources the helper is built from, the same in iyw-claw
/// and the helper when both are built from one tree (see `build.rs`). The
/// helper says it in [`HelperReady::source`]; a development iyw-claw checks it.
pub const SOURCE_FINGERPRINT: &str = env!("IYW_CLAW_COMPUTER_SOURCE");

/// iyw-claw → helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperRequest {
    pub id: u64,
    pub op: HelperOp,
    /// How many of the person's Stops iyw-claw had counted when it let this
    /// request through. The helper serves nothing of a request from before
    /// a Stop it has heard of ([`HelperOp::Halt`]) — whichever of the two
    /// frames reached it first — and holds nothing against one from after.
    pub stop: u64,
}

/// The `stop` of a [`HelperOp::Halt`] that ends everything: iyw-claw is closing
/// the helper, and nothing it was asked before is served.
pub const STOP_ALL: u64 = u64::MAX;

/// What iyw-claw may ask the helper for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HelperOp {
    /// Where the driver is, and which release it should be. Nothing that
    /// needs the driver is served before this; the helper checks the file
    /// against the pins compiled into it, not against anything said here.
    #[serde(rename_all = "camelCase")]
    Configure {
        driver_path: String,
        driver_version: String,
    },
    /// The helper's own OS permissions. Read-only: never raises a dialog.
    /// (Asking for one is not an op: iyw-claw starts a helper of its own for
    /// that — see [`REQUEST_PERMISSION_ARG`].)
    Permissions,
    ListApps,
    /// The installed application listed under `key` — its bundle identifier
    /// or path, as `list_apps` gives them — or else `name`, any case: the one
    /// application, running or not, that goes by it.
    #[serde(rename_all = "camelCase")]
    FindApp {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    /// Start an application [`HelperOp::FindApp`] found, in the background,
    /// by what the driver listed it under — never by anything an agent
    /// wrote.
    #[serde(rename_all = "camelCase")]
    LaunchApp {
        app: InstalledApp,
    },
    /// Every normal window, or only `pid`'s.
    #[serde(rename_all = "camelCase")]
    ListWindows {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
    /// When `pid` started, if it is running — the other half of a process's
    /// identity, since pids are reused.
    #[serde(rename_all = "camelCase")]
    ProcessStart {
        pid: u32,
    },
    #[serde(rename_all = "camelCase")]
    Capture {
        pid: u32,
        window_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_dimension: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    Snapshot {
        pid: u32,
        window_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_elements: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        /// macOS: keep the application's menu bars in the tree, and their
        /// elements actionable — for an application shared as a whole. A
        /// window shared on its own is read without them: they act on the
        /// whole application.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        app_menus: bool,
    },
    #[serde(rename_all = "camelCase")]
    Verify {
        pid: u32,
        window_id: u64,
        request: VerifyRequest,
    },
    /// Act on one window. iyw-claw has checked the grant, the addressing, the
    /// keys and — for the front — that the person allows it; the helper
    /// checks again what only it can see at the moment of delivery — that
    /// the pid is still the process the window was shared from, and the
    /// process drawing inside it still the run it was shared with, that the
    /// session is not locked, that no Stop has come since the action was let
    /// through — and refuses secret fields itself.
    #[serde(rename_all = "camelCase")]
    Act {
        pid: u32,
        window_id: u64,
        /// The process start time the grant is held against.
        started_at: u64,
        /// The run of the process drawing inside the window, where that is
        /// not its owner (`RawWindow::content`): part of what was shared.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<ProcessRun>,
        /// The application's key (bundle identifier or path), for the driver
        /// paths that differ by application.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_key: Option<String>,
        action: WindowAction,
        /// In the background, or with the window brought to the front for
        /// it.
        #[serde(default)]
        delivery: ActDelivery,
        /// What the action has to do with the clipboard.
        #[serde(default)]
        clipboard: ClipboardUse,
    },
    /// The entire screen, as one picture: every window whose application
    /// may not be shared by `rules` painted over, whatever layer it is on.
    #[serde(rename_all = "camelCase")]
    CaptureScreen {
        rules: ScreenRules,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_dimension: Option<u32>,
    },
    /// Have the driver running, starting it if it is not: what an action
    /// on the entire screen waits for before it is asked about a last time
    /// and sent ([`HelperOp::ActScreen`] starts none).
    DriverReady,
    /// A pointer action on the entire screen — a click, a drag, a scroll —
    /// at points in its picture's own pixels, sent as real input at the
    /// front. No point may be on what the picture paints over by `rules`,
    /// and the screen must still be as the picture was taken (`geometry`).
    /// Only on a driver already running ([`HelperOp::DriverReady`]): one
    /// started now would take seconds no one is asked about again.
    #[serde(rename_all = "camelCase")]
    ActScreen {
        rules: ScreenRules,
        action: WindowAction,
        geometry: ScreenGeometry,
    },
    /// Read the clipboard — only while it is still as `expect` names it:
    /// what an agent put there itself. Never a clipboard an application
    /// marked concealed.
    #[serde(rename_all = "camelCase")]
    ClipboardRead {
        expect: u64,
    },
    /// Put text on the clipboard; answers with the clipboard's stamp after.
    #[serde(rename_all = "camelCase")]
    ClipboardWrite {
        text: String,
    },
    /// The person pressed Stop — iyw-claw's `stop`-th — or iyw-claw is closing the
    /// helper ([`STOP_ALL`]): kill the driver started for a request from
    /// before it, now, whatever it is in the middle of, and serve nothing
    /// more of any request from before it. A `Halt` older than one already
    /// heard changes nothing, and what comes after it runs as usual, on a
    /// fresh driver: a Stop ends what is under way, not computer use.
    #[serde(rename_all = "camelCase")]
    Halt {
        stop: u64,
    },
}

/// The argument that starts the helper for one permission request instead
/// of serving iyw-claw: `iyw-computer-helper --request-permission
/// accessibility`. iyw-claw starts it as it starts the helper — its own TCC
/// principal — so the request names the helper; and a fresh process each
/// time, because macOS takes a request from each process once. It asks for
/// that permission alone, then prints a [`PermissionAsked`] line and exits.
pub const REQUEST_PERMISSION_ARG: &str = "--request-permission";

/// What a `--request-permission` helper prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAsked {
    /// The system put up its own request, which has a button to the right
    /// pane of System Settings. It does not once the person has turned the
    /// switch off there — and never for a permission already granted.
    pub prompted: bool,
}

/// An element of a snapshot the helper took, by the driver's snapshot id and
/// the element's index in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementRef {
    pub snapshot_id: String,
    pub index: u32,
}

/// A point in the window, in its own pixels — the space of a full-size
/// capture of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPoint {
    pub x: f64,
    pub y: f64,
    /// The window's size (its bounds, in the platform's units) when the point
    /// was read off it. The window at another size is laid out otherwise, and
    /// the helper refuses rather than click where the point used to be.
    pub window_width: f64,
    pub window_height: f64,
}

/// Where a pointer action lands, as the helper is told it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "at", rename_all = "camelCase")]
pub enum DriverTarget {
    Element(ElementRef),
    Point(WindowPoint),
}
