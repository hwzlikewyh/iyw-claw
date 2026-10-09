// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether `app`'s windows may be shared at all.
pub fn grantable(
    app: &RawApp,
    me: &SelfIdentity,
    blocklist: &Blocklist,
) -> Result<(), NotGrantable> {
    if me.owns(app) {
        return Err(NotGrantable::OwnApp);
    }
    if blocklist.matches(app) {
        return Err(NotGrantable::Blocklisted);
    }
    // A grant is bound to (pid, start time, window): without the start time
    // it would pass to whatever the system hands that pid next, and without a
    // key the blocklist above could not have matched it.
    if app.started_at.is_none() || app.key().is_none() {
        return Err(NotGrantable::Unidentified);
    }
    Ok(())
}

/// Why a window's grant changed.
///
/// The current level travels on `computer://state` with the rest of the shared
/// window, so this is not a second source of truth for it. It carries what the
/// state cannot: that a change was not the user's doing, and what happened
/// instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantChange {
    /// The user shared the window, or changed the level.
    Granted,
    /// The user took it back.
    Revoked,
    /// The window closed, or the process that owned it is gone — including a
    /// relaunch, which is a different process and not the one that was
    /// shared.
    TargetChanged,
    /// Unused for longer than the grant timeout.
    Expired,
    /// The user switched computer use off, which ends every grant.
    Disabled,
    /// The user pressed Stop, which ends every grant at once.
    Stopped,
}

/// `computer://agent-grant`: a transition, with its reason. Current state:
/// `computer://state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerGrantPayload {
    pub target_id: String,
    pub change: GrantChange,
    pub level: GrantLevel,
}

pub const AGENT_GRANT_EVENT: &str = "computer://agent-grant";

/// What an agent did to a window, for the person watching.
///
/// One variant per kind of touch, as on the browser's strip: the list
/// collapses runs of the same line, and forty clicks should not swallow the
/// one keystroke among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComputerAction {
    /// Took a screenshot.
    Capture,
    /// Read the accessibility tree.
    Snapshot,
    /// Checked predicates against it.
    Verify,
    Click,
    /// Dragged from one point to another.
    Drag,
    Scroll,
    /// Typed text into an element.
    Type,
    /// Pressed a key.
    Key,
    /// Held a key down.
    HoldKey,
    /// Set an element's value outright.
    SetValue,
    /// Put a minimized window back on the screen.
    Restore,
    /// Chose a command from the application's menus.
    Menu,
    /// Moved or sized the window.
    SetFrame,
    /// Started an application.
    Launch,
    /// Read back what an agent put on the clipboard.
    ClipboardRead,
    /// Put text on the clipboard.
    ClipboardWrite,
}
