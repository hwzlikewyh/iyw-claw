// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The application a window belongs to, as an agent may name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAppRef {
    /// The stable name of the application: its bundle identifier on macOS,
    /// its executable path elsewhere. What a blocklist entry matches.
    pub key: String,
    /// What the application calls itself, for a person to recognise.
    pub name: String,
    pub pid: u32,
}

/// One running application, as `computer_list_apps` reports it.
///
/// Not behind a grant: which applications are running is what is on the
/// desktop, not what any of them shows. The group switch is what decides
/// whether an agent may ask at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAppSummary {
    #[serde(flatten)]
    pub app: AgentAppRef,
    /// Whether it is the frontmost application.
    pub active: bool,
    /// What it is shared for as a whole application — every window of it,
    /// its menus and its own shortcuts — when the user shared it so.
    #[serde(default, skip_serializing_if = "not_shared")]
    pub level: GrantLevel,
    /// Why none of its windows can be shared, when that is so — iyw-claw itself,
    /// or an application on the blocklist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One native window, as `computer_list_windows` reports it.
///
/// A listing exists so an agent can *name* a window — to read it, or to ask
/// the user to share it — which is why it is not itself behind a grant. What
/// it carries is bounded by that purpose.
///
/// The title is the exception, for the same reason the browser withholds a
/// tab's: it is chosen by the application and is the first line of its
/// content. A mail client's window called "Re: termination letter" hands over
/// exactly what the grant exists to withhold. So it appears only once the
/// window is readable, when the agent could have read the whole window anyway
/// and is merely saved a round trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWindowSummary {
    /// iyw-claw's own name for this window. Stable for as long as the window and
    /// the process that owns it are the same ones; a new process — even the
    /// same application relaunched — gets a new id.
    pub target_id: String,
    pub app: AgentAppRef,
    pub bounds: Rect,
    pub on_screen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimized: Option<bool>,
    /// Its application is hidden (macOS ⌘H): off the screen as a whole,
    /// with nothing of the window's own changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    pub level: GrantLevel,
    /// Shared with its whole application: its menus and its own shortcuts
    /// are in reach too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub whole_app: bool,
    /// Shared with the entire screen: its application's menus, and the
    /// desktop's own shortcuts but locking and logging out, are in reach too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub whole_screen: bool,
    /// Present only from [`GrantLevel::Read`] upwards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Why this window cannot be shared, when that is so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The entire screen in a listing, while the user shares it as a whole: a
/// target of its own (`targets::SCREEN_TARGET_ID`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentScreen {
    pub target_id: String,
    pub level: GrantLevel,
}

/// A screenshot of one shared window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowCapture {
    pub target_id: String,
    /// Names this capture: `<grant epoch>.<read number>`. Coordinates read
    /// off this image mean something only in this image's pixel space, and
    /// the actions that arrive later will carry the generation of the image
    /// their coordinates came from.
    pub generation: String,
    /// `image/png`.
    pub mime: String,
    /// The image, base64.
    pub data: String,
    /// The image's size in pixels — the space coordinates read off it are in.
    pub width: u32,
    pub height: u32,
    /// Where the window was when it was captured, in desktop coordinates.
    pub window_bounds: Rect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// An accessibility snapshot of one shared window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSnapshot {
    pub target_id: String,
    /// See [`WindowCapture::generation`].
    pub generation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_bounds: Option<Rect>,
    /// The tree, as indented text, one element per line.
    pub tree: String,
    /// How many actionable elements the driver found, before any cut.
    pub element_count: u64,
    /// The tree was cut short, by `maxChars` or by the driver's own bounds.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// Why the tree is empty or partial when the window is not (a canvas, a
    /// window whose accessibility surface the driver could not resolve), in
    /// the driver's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded: Option<String>,
}

/// One predicate `computer_verify` checks against a shared window. The
/// driver ANDs them.
///
/// A closed set: every field the agent may send is named here, and the
/// request is rebuilt from these types before it reaches the driver, so
/// nothing the agent writes is forwarded as-is.
///
/// There is deliberately no "value equals" predicate yet. A yes / no answer
/// about a field's value is a way to read that value one guess at a time, and
/// the fields worth guessing at are the secure ones the snapshot refuses to
/// show; the check that makes it safe (refusing selectors that can match a
/// secure field) comes with the action tools.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyPredicate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPredicate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<ElementPredicate>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowPredicate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<BoundsPredicate>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundsPredicate {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance_px: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementPredicate {
    pub selector: ElementSelector,
    /// Only `true`: absence cannot be proven on every platform, and the
    /// driver refuses `false` rather than answer "unknown" forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_contains: Option<String>,
}

/// The largest number of predicates one `computer_verify` may carry — the
/// driver's own bound, checked here so the agent is told in our words.
pub const MAX_VERIFY_PREDICATES: usize = 8;

/// What `computer_verify` asks for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub expect: Vec<VerifyPredicate>,
    /// How long to keep sampling, in milliseconds. Zero samples once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    /// How many consecutive satisfied samples count as success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_samples: Option<u32>,
}

/// A predicate's answer, or the whole check's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyStatus {
    Satisfied,
    Unsatisfied,
    /// Could not be decided. Never a success: an agent that reads "unknown"
    /// as "probably fine" is exactly the failure this tool exists to prevent.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredicateResult {
    pub index: u32,
    pub status: VerifyStatus,
    /// Why a predicate is `unknown`, in the driver's vocabulary
    /// (`target_missing`, `stability_unproven`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_reason: Option<String>,
}
