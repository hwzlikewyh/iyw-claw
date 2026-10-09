// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What an agent asks to do to one shared window. A closed set, rebuilt field
/// by field on its way to the driver, like the verify predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ComputerActRequest {
    /// Click an element, or a point. `count` is 1 or 2 (a double click, left
    /// button only). `modifiers` are held down for the click.
    #[serde(rename_all = "camelCase")]
    Click {
        target: AgentTarget,
        #[serde(default)]
        button: PointerButton,
        count: u8,
        #[serde(default, skip_serializing_if = "no_modifiers")]
        modifiers: crate::computer::keys::Modifiers,
    },
    /// Press at one point of the window's latest screenshot, move to another
    /// and let go — with `modifiers` held for the whole of it. `duration_ms`
    /// is how long the path takes.
    #[serde(rename_all = "camelCase")]
    Drag {
        from: PointTarget,
        to: PointTarget,
        #[serde(default)]
        button: PointerButton,
        #[serde(default, skip_serializing_if = "no_modifiers")]
        modifiers: crate::computer::keys::Modifiers,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    /// Scroll at an element or a point — or, with no target, whatever has
    /// focus in the window.
    #[serde(rename_all = "camelCase")]
    Scroll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<AgentTarget>,
        direction: ScrollDirection,
        amount: u32,
        #[serde(default)]
        unit: ScrollUnit,
    },
    /// Type text into an element; `submit` presses return after it.
    #[serde(rename_all = "camelCase")]
    Type {
        target: ElementTarget,
        text: String,
        #[serde(default)]
        submit: bool,
    },
    /// Press a key, `repeat` times, on an element or on whatever has focus.
    #[serde(rename_all = "camelCase")]
    Key {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<ElementTarget>,
        chord: crate::computer::keys::Chord,
        repeat: u32,
    },
    /// Hold a key down for `duration_ms`, as a held key repeats: pressed,
    /// then — after the system's usual delay — again and again until the
    /// time is up. On an element or on whatever has focus.
    #[serde(rename_all = "camelCase")]
    HoldKey {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<ElementTarget>,
        chord: crate::computer::keys::Chord,
        duration_ms: u32,
    },
    /// Set an element's value outright — a text field's text, a slider's
    /// position, a pop-up menu's choice.
    #[serde(rename_all = "camelCase")]
    SetValue {
        target: ElementTarget,
        value: String,
    },
    /// Put a window back on the screen — out of the Dock or the taskbar if it
    /// is minimized, its application shown again if it is hidden: typing,
    /// keys, scrolling, a point and a screenshot all need it there.
    Restore,
    /// Choose a command from the application's menus, by the titles on the
    /// way to it: `["File", "Export", "PDF…"]`. The application's own, so
    /// only for an application shared as a whole.
    #[serde(rename_all = "camelCase")]
    InvokeMenu { path: Vec<String> },
    /// Move and size the window, in desktop units as listings give a
    /// window's bounds; what is left out stays as it is.
    #[serde(rename_all = "camelCase")]
    SetFrame {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
    },
}

/// The smallest a window may be made, either way, in desktop units.
pub const MIN_WINDOW_SIDE: f64 = 50.0;

/// The largest a window may be made, either way, and the furthest from the
/// desktop's origin it may be put.
pub const MAX_WINDOW_EXTENT: f64 = 100_000.0;
