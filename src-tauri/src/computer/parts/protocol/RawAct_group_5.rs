// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What the helper reports of an action that went out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawAct {
    pub effect: ActEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<ActRoute>,
    /// For typing with `submit`: whether return was pressed after the text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted: Option<bool>,
    /// For typing with `submit` whose return did not go out: why, in words
    /// for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_note: Option<String>,
    /// Where the action was aimed, as the helper knew it when it went out:
    /// the element's frame in the snapshot it was addressed by, and — for a
    /// point — the window's frame, measured just before. In the platform's
    /// desktop units. For showing the person where an agent acted; nothing
    /// is decided by them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_frame: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_frame: Option<Rect>,
    /// For an action that copies ([`ClipboardUse::track`]): the clipboard's
    /// stamp once the action changed it — what the agent itself put there.
    /// `None` when it did not change in time, or holds what an application
    /// marked concealed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<u64>,
}

/// Who may never be seen or touched over the entire screen: iyw-claw itself,
/// and the applications on the blocklist — which the helper judges on its
/// own there, window by window, as iyw-claw judges a window it shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenRules {
    pub me: crate::computer::agent::SelfIdentity,
    pub blocklist: Vec<String>,
}

/// The screen as a picture of it was taken: what a point read off the
/// picture means.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenGeometry {
    /// The picture's own pixels to a desktop unit.
    pub scale: f64,
    /// The screen's size, in desktop units.
    pub width: f64,
    pub height: f64,
}

/// What an action has to do with the clipboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardUse {
    /// The action copies or cuts, or may (a menu command): watch whether it
    /// changes the clipboard, and say what to.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub track: bool,
    /// The clipboard as the agent last put it there itself, when iyw-claw holds
    /// that it still may be pasted: an action that pastes — a paste key, a
    /// menu command or a control that pastes — goes only while the clipboard
    /// is still this. Without it, nothing that pastes goes at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste: Option<u64>,
}

/// What the clipboard holds, read for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawClipboard {
    /// Its text, when it has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// helper → iyw-claw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HelperMessage {
    /// The helper's first frame. See the module note for why it goes first.
    Ready(HelperReady),
    Reply(HelperReply),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperReady {
    pub protocol: u32,
    /// The helper's own crate version, which is iyw-claw's.
    pub version: String,
    /// What the helper knows about who it is talking to.
    pub peer: PeerCheck,
    /// [`SOURCE_FINGERPRINT`] as the helper was built. Absent from a helper
    /// older than the field, which a development iyw-claw takes for a stale one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Whether the helper checked iyw-claw's code signature before serving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PeerCheck {
    /// It did, and iyw-claw passed (macOS release builds).
    Verified,
    /// A development build: no requirement was compiled in to check against.
    /// Said out loud so iyw-claw can show it.
    Development,
    /// No code signature to check on this platform.
    NotApplicable,
}

/// The answer to one [`HelperRequest`]: exactly one of `ok` / `error`.
///
/// `ok` travels as a plain JSON value because iyw-claw knows which op it asked
/// and decodes it into that op's type ([`HelperReply::decode`]); a tagged
/// union here would only repeat the op kind the id already names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperReply {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<HelperError>,
}
