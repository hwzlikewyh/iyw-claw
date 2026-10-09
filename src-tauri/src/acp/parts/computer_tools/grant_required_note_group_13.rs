// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A refusal about one window, shared by the three per-window reads.
///
/// The words are the same whichever read was refused, so a refusal cannot be
/// used to learn which gate a window stops at.
pub fn grant_required_note(target_id: &str) -> String {
    format!(
        "Window {target_id} is not shared with agents, or no longer is (sharing ends when the \
         window closes, when its application quits, after a while unused, or when the user takes \
         it back). {SHARE_HOW} If the window may have closed, call computer_list_windows first."
    )
}

pub fn no_such_target_note(target_id: &str) -> String {
    format!(
        "There is no window {target_id}. Call computer_list_windows for the windows that exist \
         now, and use a targetId from it."
    )
}

pub fn blocked_note(target_id: &str, why: &str) -> String {
    format!("Window {target_id} is {why} Do not ask the user to share it; it cannot be done.")
}

pub fn permission_missing_note(permission: &str) -> String {
    format!(
        "iyw-computer-helper has not been granted {permission} by macOS. Ask the user to grant \
         it: in iyw-claw's status bar they open Computer use and follow the permission guide \
         (System Settings → Privacy & Security → {permission}). Only they can, and retrying will \
         not help until they have."
    )
}

pub fn control_required_note(target_id: &str) -> String {
    format!(
        "Window {target_id} is shared with you for reading only. Ask the user to let you act on \
         it: in iyw-claw's status bar they open Computer use and set that window to \"Read and \
         act\". Only they can; retrying will not change it. You can still read the window."
    )
}

/// A key that reaches past the window — the application's or the desktop's.
pub fn chord_beyond_note() -> String {
    format!(
        "That key acts on the whole application or on the desktop, which a shared window does \
         not reach, so it was not pressed; retrying will not change it. {} For anything else, \
         act on an element: computer_click by ref, or computer_set_value. An application the \
         user shares as a whole takes its own shortcuts too, and its menus \
         (computer_invoke_menu) — that is theirs to choose.",
        crate::computer::keys::window_chords_note(crate::computer::keys::Platform::current())
    )
}

/// Said when keys are to be held over a double-click in a window: the
/// driver's double-click does not hold them.
pub const DOUBLE_CLICK_MODIFIERS_NOTE: &str = "Holding keys down over a double-click is not \
     available in a window, so nothing was sent. Double-click without `modifiers`, or reach the \
     same end another way.";

/// Said when keys are to be held over a drag on a system whose driver would
/// drag without them.
pub const DRAG_MODIFIERS_NOTE: &str = "Holding keys down over a drag is not available on this \
     system: the drag would go without them, so nothing was sent. Drag without `modifiers`, or \
     reach the same end another way.";

/// Said when the window was taken back between two presses of one key and
/// shared again before the next: the presses end with the sharing they began
/// under.
pub fn reshared_note(target_id: &str) -> String {
    format!(
        "Window {target_id} was taken back from you while the key was being pressed, which \
         ended the presses; the user has shared it again since. Look at the window again before \
         going on."
    )
}

/// Said when a key is the desktop's own, under a grant on the whole
/// application.
pub const DESKTOP_CHORD_NOTE: &str = "That key is the desktop's own — it switches applications, \
     opens the launcher, takes a screenshot, locks the screen or the like — which not even an \
     application shared as a whole reaches, so it was not pressed; retrying will not change it.";

/// Said when a key locks the screen, logs out, or shows every window at
/// once.
pub const SESSION_CHORD_NOTE: &str = "That key locks the screen, logs out, or shows every window \
     at once (Mission Control, Task View) — which no sharing reaches, not even the entire \
     screen's — so it was not pressed; retrying will not change it.";

/// Said when the entire screen is asked for and it is not shared, or no
/// longer is.
pub const SCREEN_GRANT_REQUIRED_NOTE: &str =
    "The entire screen (d1) is not shared with agents, or \
     no longer is (that sharing ends after a while unused, or when the user takes it back or \
     switches it off). Ask the user to share it: in iyw-claw's status bar they open Computer use, \
     press \"Share a window…\" and choose the entire screen — only they can, and it is offered \
     only where they have switched it on in iyw-claw's Computer use settings. Meanwhile, read the \
     windows shared with you one by one.";

/// Said when the entire screen is shared for reading and an action is asked.
pub const SCREEN_CONTROL_REQUIRED_NOTE: &str = "The entire screen (d1) is shared with you for \
     reading only. Ask the user to let you act on it: in iyw-claw's Computer use panel they set the \
     entire screen to \"Read and act\". Only they can; retrying will not change it.";

/// Said when a point on the entire screen is not from its latest picture.
pub const SCREEN_STALE_CAPTURE_NOTE: &str = "Those coordinates are not from the latest \
     computer_screenshot of the entire screen (d1): a point means something only in the picture \
     it was read off. Take a new computer_screenshot of d1 and use a point from it.";

/// Said when the entire screen is asked for something other than its
/// picture and points on it.
pub const SCREEN_POINTER_ONLY_NOTE: &str = "The entire screen (d1) takes computer_screenshot, and \
     computer_click, computer_drag and computer_scroll at points of its picture. For anything \
     else — keys, typing, a value, a menu, the controls' tree — act on a window: every window the \
     user can share is shared with you along with the screen (see computer_list_windows).";

/// Said when an action on the entire screen is asked for and the person
/// does not allow the front.
pub const SCREEN_NEEDS_FRONT_NOTE: &str = "An action on the entire screen moves the user's own \
     pointer and goes to whatever is in front at that point, and the user has switched off \
     bringing windows to the front in iyw-claw's Computer use settings (\"Let agents bring windows \
     to the front\"), so nothing was sent. Act on a window in the background instead, or ask the \
     user whether to switch it back on — only they can.";

/// Said when an action on the entire screen is asked to go in the
/// background.
pub const SCREEN_NOT_BACKGROUND_NOTE: &str = "An action on the entire screen goes to the front, \
     as the user's own pointer would — never in the background — so nothing was sent. Leave \
     `delivery` out, or act on a window in the background instead.";

/// Said when the never-share list grew while the entire screen was being
/// captured.
pub const SCREEN_RULES_CHANGED_NOTE: &str = "The user's never-share list changed while the entire \
     screen was being captured, so the picture was not handed over. Take a new computer_screenshot \
     of d1.";

/// Said when something only an application shared as a whole allows is
/// asked of a window shared on its own.
pub fn app_grant_required_note(target_id: &str) -> String {
    format!(
        "Menus act on the whole application, and window {target_id} is shared on its own. Ask \
         the user to share its application as a whole, for \"Read and act\", in iyw-claw's Computer \
         use panel — only they can."
    )
}

/// Said for a menu command on Windows, whose driver cannot choose one.
pub const MENUS_UNAVAILABLE_NOTE: &str = "Menus cannot be chosen by title on Windows, so nothing \
     was sent. Take a computer_snapshot and click the menu, then its item, by ref.";

/// Said when a menu command is asked for and the person does not allow
/// bringing windows to the front.
pub const MENU_NEEDS_FRONT_NOTE: &str = "A menu command is chosen with its application brought to \
     the front, and the user has switched that off in iyw-claw's Computer use settings (\"Let agents \
     bring windows to the front\"), so nothing was sent. Ask the user whether to switch it back \
     on — only they can.";

/// Said when starting an application, or moving a window, is asked for and
/// the person has not switched it on.
pub const LAUNCH_OFF_NOTE: &str = "Starting applications and moving or sizing windows is \
     switched off in iyw-claw's Computer use settings (\"Let agents open applications and move \
     windows\"), so nothing was done. Ask the user whether to switch it on — only they can.";

/// Said when the clipboard tools are asked for and the person has not
/// switched them on.
pub const CLIPBOARD_OFF_NOTE: &str = "Reading and writing the clipboard is switched off in \
     iyw-claw's Computer use settings (\"Let agents use the clipboard\"), so nothing was done. Ask \
     the user whether to switch it on — only they can.";

/// Said when the clipboard does not hold what an agent put there.
pub const CLIPBOARD_NOT_YOURS_NOTE: &str = "The clipboard holds what the user put there, not what \
     you copied from a window you may read or wrote with computer_clipboard_write, so it is not \
     read for you; retrying will not change it. Copy from a shared window first.";

/// Said with text put on the clipboard for an agent.
pub const CLIPBOARD_WRITTEN_NOTE: &str =
    "The text is on the clipboard: a paste into a window shared \
     with you for control now writes it there, until something else is copied.";

/// Said with an application started for an agent.
pub const LAUNCHED_NOTE: &str = "It was started in the background. Its windows are not shared \
     with you by this: find them with computer_list_windows, and ask the user to share the one \
     you need — only they can.";

/// Said for a frame no window can have.
pub const BAD_FRAME_NOTE: &str = "That frame cannot be given to a window: every number must be a \
     plain number, the width and the height at least 50, and nothing beyond 100000. Use \
     desktop units, as computer_list_windows gives a window's bounds.";

pub const PASTE_NOTE: &str = "That pastes, and the clipboard holds what the user put there — not \
     what you copied from a window you may read, or wrote with computer_clipboard_write — so it \
     was not pressed. Type the text with computer_type instead, or copy it from a shared window \
     first.";

pub const NEEDS_ELEMENT_NOTE: &str = "A key that types a character goes only into an element you \
     name: pass its ref from computer_snapshot, or type the text with computer_type.";

pub const SECRET_FIELD_NOTE: &str = "That is a password or other secret field: typing into it, or \
     setting it, is left to the user. Ask them to fill it in themselves; retrying will not change \
     it.";

pub fn stale_snapshot_note(target_id: &str) -> String {
    format!(
        "That ref is not from the latest computer_snapshot of window {target_id} — every new \
         snapshot replaces the refs of the one before. Take a new computer_snapshot and use a ref \
         from it."
    )
}

pub fn not_actionable_note(target_id: &str) -> String {
    format!(
        "Nothing in that snapshot of window {target_id} can be acted on: its accessibility tree \
         could not be matched to the window. Take a new computer_snapshot; if it says the same, \
         use a point from computer_screenshot instead."
    )
}

pub fn cut_away_note(index: u32) -> String {
    format!(
        "Ref {index} was past where the snapshot you were given was cut (maxChars). Take a new \
         computer_snapshot with a larger maxChars, or a query that keeps its line, and use the \
         ref from that."
    )
}

pub fn no_such_ref_note(target_id: &str, index: u32) -> String {
    format!(
        "The latest snapshot of window {target_id} has no ref {index}. Use a ref that is in it."
    )
}

pub fn stale_capture_note(target_id: &str) -> String {
    format!(
        "Those coordinates are not from the latest computer_screenshot of window {target_id}: a \
         point means something only in the image it was read off. Take a new computer_screenshot \
         and use a point from it."
    )
}

pub const OUT_OF_IMAGE_NOTE: &str = "That point is outside the screenshot it names. Use a point \
     inside the image, measured in its pixels from its top-left corner.";
