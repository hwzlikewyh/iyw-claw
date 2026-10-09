// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// One action on one window: the closed list the helper translates into
/// driver calls. Delivered as its [`HelperOp::Act`] says — in the background
/// unless the person allows the front and it was asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WindowAction {
    #[serde(rename_all = "camelCase")]
    Click {
        at: DriverTarget,
        button: PointerButton,
        count: u8,
        /// Held down for the click.
        #[serde(default, skip_serializing_if = "no_modifiers")]
        modifiers: Modifiers,
    },
    /// Press at `from`, move to `to` over `duration_ms`, let go — with
    /// `modifiers` held for the whole of it.
    #[serde(rename_all = "camelCase")]
    Drag {
        from: WindowPoint,
        to: WindowPoint,
        button: PointerButton,
        #[serde(default, skip_serializing_if = "no_modifiers")]
        modifiers: Modifiers,
        duration_ms: u32,
    },
    #[serde(rename_all = "camelCase")]
    Scroll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<DriverTarget>,
        direction: ScrollDirection,
        amount: u32,
        unit: ScrollUnit,
    },
    #[serde(rename_all = "camelCase")]
    Type {
        element: ElementRef,
        text: String,
        submit: bool,
    },
    #[serde(rename_all = "camelCase")]
    Key {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        element: Option<ElementRef>,
        chord: Chord,
    },
    #[serde(rename_all = "camelCase")]
    SetValue { element: ElementRef, value: String },
    /// Put the window back on the screen: out of the Dock or the taskbar if
    /// it is minimized, and on macOS its application shown again if it is
    /// hidden. The helper's own on macOS (Accessibility) and Windows; on
    /// Linux the driver's, which can only do it by bringing the window to
    /// the front — so there it goes only with [`ActDelivery::Foreground`].
    Restore,
    /// Choose a command from the application's menus, by the titles on the
    /// way to it. The driver's, with the application brought to the front
    /// for it (macOS and Linux).
    #[serde(rename_all = "camelCase")]
    InvokeMenu { path: Vec<String> },
    /// Move and size the window, in desktop units: what is given put in
    /// place of the window's frame as the helper finds it just before.
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
