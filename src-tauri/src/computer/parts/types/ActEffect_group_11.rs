// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How far the driver can vouch for an action it carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActEffect {
    /// Read back from the window: the value changed, the selection moved.
    Confirmed,
    /// Only part of it happened (some of the text was typed).
    Partial,
    /// Delivered, and nothing could be read back to prove it landed. Not a
    /// failure and not a success: look at the window to know.
    Unverifiable,
    /// Delivered, and by every sign it did nothing.
    SuspectedNoop,
}

/// How an action reached the application — not the same click by every
/// route: a semantic accessibility action does not move the pointer or fire
/// hover, a synthesized event does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActRoute {
    Accessibility,
    SyntheticEvents,
    GlobalInput,
    SystemApi,
    Dom,
    TrustedInput,
    /// A route this iyw-claw does not know by name.
    #[serde(other)]
    Other,
}

/// How the input reaches the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActDelivery {
    /// The window is not brought to the front, and the person's own pointer
    /// and keyboard focus stay where they are.
    #[default]
    Background,
    /// The window is brought to the front for the one action, which then
    /// goes in as real input, and the window the person was in is brought
    /// back after it. Some applications take keys and typing no other way —
    /// on Windows, every one built on Chromium. Only where the person has
    /// allowed it in the settings.
    Foreground,
}
