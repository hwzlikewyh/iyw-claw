// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub const KEY_COMPUTER_TOOLS_ENABLED: &str = "computer_tools.enabled";

/// Minutes a shared window may go unread before its sharing ends; `0` is
/// "until the user takes it back".
pub const KEY_COMPUTER_TOOLS_GRANT_TTL_MINUTES: &str = "computer_tools.grant_ttl_minutes";

/// Applications the user added to the default blocklist, as a JSON array of
/// bundle identifiers, paths or executable names.
pub const KEY_COMPUTER_TOOLS_BLOCKLIST: &str = "computer_tools.blocklist";

/// Default blocklist entries the user took off it, as a JSON array of their
/// keys (`computer::agent::DEFAULT_BLOCKLIST`). Only the removals are kept,
/// not the list they leave, so an entry a later release adds to the defaults
/// is on everyone's list.
pub const KEY_COMPUTER_TOOLS_BLOCKLIST_REMOVED: &str = "computer_tools.blocklist_removed";

/// The shortcut that stops every agent at once, spelled as
/// `computer::stop_shortcut` spells it; empty is "none". Absent is the
/// platform's default.
pub const KEY_COMPUTER_TOOLS_STOP_SHORTCUT: &str = "computer_tools.stop_shortcut";

/// Whether the strip above every window comes up while anything is shared,
/// `true` or `false`. Absent is `true`.
pub const KEY_COMPUTER_TOOLS_SHOW_INDICATOR: &str = "computer_tools.show_indicator";

/// Whether an agent may have a shared window brought to the front for an
/// action, `true` or `false`. Absent is `true`: some applications take keys
/// no other way, and the person can switch it off.
pub const KEY_COMPUTER_TOOLS_ALLOW_FOREGROUND: &str = "computer_tools.allow_foreground";

/// How an action reaches its window when the agent does not say,
/// `background` or `foreground` — the second in force only while the front
/// is allowed at all. Absent is `background`.
pub const KEY_COMPUTER_TOOLS_DEFAULT_DELIVERY: &str = "computer_tools.default_delivery";

/// Whether an agent may start applications and move or size a shared
/// window, `true` or `false`. Absent is `false`: it changes the person's
/// desktop beyond the windows they shared.
pub const KEY_COMPUTER_TOOLS_LAUNCH_ENABLED: &str = "computer_tools.launch_enabled";

/// Whether an agent may read back what it put on the clipboard and put text
/// there, `true` or `false`. Absent is `false`.
pub const KEY_COMPUTER_TOOLS_CLIPBOARD_ENABLED: &str = "computer_tools.clipboard_enabled";

/// Whether the entire screen is offered in the share picker, `true` or
/// `false`. Absent is `false`: sharing the screen shares every window there
/// is, the ones that come up later included.
pub const KEY_COMPUTER_TOOLS_SCREEN_ENABLED: &str = "computer_tools.screen_enabled";

/// The grant timeout when the user has chosen none.
pub const DEFAULT_GRANT_TTL_MINUTES: u32 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComputerToolsSettings {
    pub enabled: bool,
    #[serde(default = "default_ttl")]
    pub grant_ttl_minutes: u32,
    #[serde(default)]
    pub blocklist: Vec<String>,
    /// Keys of the default entries taken off the list.
    #[serde(default)]
    pub blocklist_removed: Vec<String>,
    /// The default list as this platform names it — for the settings to
    /// show; read, never written.
    #[serde(default, skip_deserializing)]
    pub blocklist_defaults: Vec<DefaultBlockView>,
    /// The stop shortcut's spelling; empty when the person switched it off.
    #[serde(default = "default_stop_shortcut")]
    pub stop_shortcut: String,
    /// Whether the strip with Stop on it floats above every window while
    /// anything is shared.
    #[serde(default = "default_show_indicator")]
    pub show_indicator: bool,
    /// Whether an agent may have a window brought to the front for an action.
    #[serde(default = "default_allow_foreground")]
    pub allow_foreground: bool,
    /// What an action gets when the agent does not say; the front only while
    /// it is allowed, and kept as chosen while it is not.
    #[serde(default)]
    pub default_delivery: ActDelivery,
    /// Whether an agent may start applications and move or size a shared
    /// window.
    #[serde(default)]
    pub launch_enabled: bool,
    /// Whether an agent may read back what it put on the clipboard, and put
    /// text there.
    #[serde(default)]
    pub clipboard_enabled: bool,
    /// Whether the entire screen is offered in the share picker.
    #[serde(default)]
    pub screen_enabled: bool,
}
