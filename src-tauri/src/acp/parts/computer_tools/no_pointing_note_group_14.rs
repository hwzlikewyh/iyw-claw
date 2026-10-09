// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub fn no_pointing_note(target_id: &str) -> String {
    format!(
        "Points cannot be used on that screenshot of window {target_id}. Use a ref from \
         computer_snapshot instead."
    )
}

pub const STOPPED_NOTE: &str = "The user pressed Stop in iyw-claw's Computer use panel: every \
     window stopped being shared, and whatever was under way was cut off. Do not retry on your \
     own — tell the user, and go on only once they share a window with you again.";

pub const FOREGROUND_NOT_ALLOWED_NOTE: &str = "Bringing a window to the front for an action is \
     not allowed: the user has switched it off in iyw-claw's Computer use settings (\"Let agents \
     bring windows to the front\"), so nothing was sent. Leave `delivery` out to act in the \
     background; if only the front will do, ask the user whether to switch it back on — only \
     they can.";

/// Said when a window is to be restored on Linux — which takes bringing it
/// to the front — and the person does not allow that.
pub const RESTORE_NEEDS_FRONT_NOTE: &str = "On Linux a minimized window comes back on the screen \
     only by being brought to the front, and the user has switched that off in iyw-claw's Computer \
     use settings (\"Let agents bring windows to the front\"), so nothing was sent. Ask the user \
     to restore the window, or whether to switch that back on — only they can.";

/// What an action the application would not take in the background can try
/// next, as the person has the front set: the words that end every
/// `computer_background_unavailable` note.
pub fn background_next_step(allow_foreground: bool) -> &'static str {
    if allow_foreground {
        "The user allows bringing a window to the front: call again with `delivery: \
         \"foreground\"`, and iyw-claw brings this window forward for that one action, then switches \
         back to the window the user was in (on Linux it stays in front). They will see it \
         happen, and a click may move their pointer."
    } else {
        "The user has switched off bringing windows to the front in iyw-claw's Computer use \
         settings; if nothing else will do, ask them whether to switch it back on."
    }
}

/// What an action tool answers: what the action did, or why it did not
/// happen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerActOutcome {
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ActReport>,
    /// One of the slugs above. `None` exactly when `action` is `Some`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
