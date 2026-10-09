// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What `computer_clipboard_read` / `computer_clipboard_write` ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClipboardOp {
    Read,
    Write { text: String },
}

/// What `computer_clipboard_read` / `computer_clipboard_write` answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerClipboardOutcome {
    /// What was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The text was put on the clipboard.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub written: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
