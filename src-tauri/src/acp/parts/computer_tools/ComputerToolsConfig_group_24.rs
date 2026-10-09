// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The computer-use settings as the tool surface reads them, at injection and
/// again at call time — like the browser group, because switching it off
/// should stop the agent that is already running, not only the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerToolsConfig {
    pub enabled: bool,
    /// How long a shared window may go unread before its sharing ends. `None`
    /// is "until the user takes it back".
    pub grant_ttl: Option<Duration>,
    /// Applications the user added to the default blocklist.
    pub blocklist: Vec<String>,
    /// Keys of the default blocklist entries the user took off it
    /// (`computer::agent::DEFAULT_BLOCKLIST`).
    pub blocklist_removed: Vec<String>,
    /// The shortcut that stops every agent at once, from anywhere; `None`
    /// when the person switched it off.
    pub stop_shortcut: Option<crate::computer::stop_shortcut::StopShortcut>,
    /// Whether the strip above every window comes up while anything is
    /// shared. On unless the person turned it off: Stop is on it.
    pub show_indicator: bool,
    /// Whether an action may bring its window to the front
    /// ([`ActDelivery::Foreground`]). On unless the person turned it off.
    pub allow_foreground: bool,
    /// What an action gets when the agent does not ask, as the person chose
    /// it — in force only while they allow the front at all (see
    /// [`Self::default_delivery_in_force`]).
    pub default_delivery: ActDelivery,
    /// Whether an agent may start applications and move or size a shared
    /// window. Off unless the person turned it on.
    pub launch_enabled: bool,
    /// Whether an agent may read back what it put on the clipboard, and put
    /// text there. Off unless the person turned it on. (Pasting what it
    /// copied needs no switch: see `commands::computer`.)
    pub clipboard_enabled: bool,
    /// Whether the person may share the entire screen at once. Off unless
    /// they turned it on; turning it off ends the screen's share.
    pub screen_enabled: bool,
    /// How many times the group has been switched off since iyw-claw started.
    /// Kept by [`ComputerToolsRuntimeConfig::set`], never persisted: it is
    /// what lets a watcher that only sees the latest value — a quick off and
    /// on again arrives as one change — still see that there was an off, and
    /// a read in flight see that one happened while it was.
    pub switched_off: u64,
}
