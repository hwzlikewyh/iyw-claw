// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// This build has no desktop to show (server mode), the user has switched
/// computer use off, or it cannot run here right now (the note says which).
pub const ERROR_UNAVAILABLE: &str = "computer_unavailable";

/// The window exists and this agent may not read it: nobody shared it, the
/// sharing ended, or the window went away. One slug for all of them — the
/// instruction is the same, and telling them apart would describe a window
/// the agent has no right to know anything about.
pub const ERROR_GRANT_REQUIRED: &str = "computer_grant_required";

/// No window by that id. Also the answer to a caller whose token does not
/// check out, so an unauthenticated round trip learns nothing about what is
/// on the screen.
pub const ERROR_NO_SUCH_TARGET: &str = "computer_no_such_target";

/// The window can never be shared: iyw-claw's own, or an application on the
/// blocklist. Permanent — asking again changes nothing.
pub const ERROR_BLOCKED: &str = "computer_blocked";

/// The OS has not given iyw-claw's helper a permission the read needs. The user
/// can fix it in System Settings; the agent cannot.
pub const ERROR_PERMISSION_MISSING: &str = "computer_permission_missing";

/// The window was shared and the read still did not produce anything — the
/// driver failed, or the window closed mid-read.
pub const ERROR_READ_FAILED: &str = "computer_read_failed";

/// The window is shared for reading and the agent asked to act on it — or
/// asked for a key a window grant does not reach (one that acts on the whole
/// application or the desktop). Its own slug, like the browser's: the person
/// has a different thing to do than share the window.
pub const ERROR_CONTROL_REQUIRED: &str = "computer_control_required";

/// The ref or point is not from the window's latest snapshot or screenshot as
/// the agent was given it, or the window has changed under it. Not a
/// permission matter: read the window again and use what the new read says.
pub const ERROR_STALE_REF: &str = "computer_stale_ref";

/// The point is outside the image it was read off, or the element is not
/// part of the shared window.
pub const ERROR_OUT_OF_TARGET: &str = "computer_out_of_target";

/// The window cannot take input in the background right now — minimized,
/// hidden, on another desktop, or its application has another window the keys
/// could reach instead.
pub const ERROR_OCCLUDED: &str = "computer_occluded";

/// The application offers no background route for this action. Whether it
/// may go again with the window brought to the front is the person's to
/// allow; the note says which it is.
pub const ERROR_BACKGROUND_UNAVAILABLE: &str = "computer_background_unavailable";

/// The agent asked for the window to be brought to the front for an action
/// (`delivery: "foreground"`), and the person has not allowed that. Theirs to
/// change, in iyw-claw's settings; asking again changes nothing.
pub const ERROR_FOREGROUND_NOT_ALLOWED: &str = "computer_foreground_not_allowed";

/// The action was allowed and did not happen: a disabled control, no such
/// option, more text than one call can type. The note says which.
pub const ERROR_ACTION_FAILED: &str = "computer_action_failed";

/// The screen is locked, or another user's session is active. Nothing
/// reaches any window until it is unlocked.
pub const ERROR_PAUSED: &str = "computer_paused";

/// The person pressed Stop: every window stopped being shared, and whatever
/// was under way was cut off. Nothing is shared again until they share it.
pub const ERROR_STOPPED: &str = "computer_stopped";

/// What a `computer_snapshot` asks for when the caller names no cap — the
/// same default as `browser_snapshot`, for the same reason: the caller who
/// names nothing is a model with a context window.
pub const DEFAULT_SNAPSHOT_MAX_CHARS: usize = 40_000;

/// The long edge of a screenshot when the caller names none: the driver's own
/// default, and the size every current model accepts without resizing.
pub const DEFAULT_MAX_DIMENSION: u32 = 1568;

/// Said to an agent in a runtime with no desktop, and to one whose user has
/// switched the group off.
pub const NO_DESKTOP_NOTE: &str =
    "Computer use is not available in this session: there are no windows to read.";
