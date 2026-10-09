// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Run `task` on the runtime iyw-claw runs on: Tauri's in the desktop app,
/// which starts the service before any task of its own runs; the process's
/// own in iyw-claw-server.
pub(super) fn spawn_task(task: impl std::future::Future<Output = ()> + Send + 'static) {
    #[cfg(feature = "tauri-runtime")]
    tauri::async_runtime::spawn(task);
    #[cfg(not(feature = "tauri-runtime"))]
    tokio::spawn(task);
}

/// What an agent last put on the clipboard itself, as the clipboard was
/// stamped then: copied out of a window it may read (`source`, the window and
/// the sharing it was copied under), or written with computer_clipboard_write
/// (no source). It may be pasted, or read back, only while the clipboard is
/// still that — and, for a copy, while its window is still shared so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OwnedClipboard {
    pub(in crate::commands::computer) stamp: u64,
    pub(in crate::commands::computer) source: Option<(String, u64)>,
}

/// Whether an action could paste, and so needs the clipboard checked: a
/// paste key, a menu command, or a press of a named element — which may be
/// a control that pastes.
pub(super) fn may_paste(request: &ComputerActRequest) -> bool {
    use crate::computer::keys::{classify, ChordClass, Platform};
    match request {
        ComputerActRequest::Key { chord, target, .. }
        | ComputerActRequest::HoldKey { chord, target, .. } => {
            target.is_some() || classify(chord, Platform::current()) == ChordClass::Paste
        }
        ComputerActRequest::InvokeMenu { .. } => true,
        ComputerActRequest::Click { target, .. } => {
            matches!(target, crate::computer::types::AgentTarget::Element(_))
        }
        _ => false,
    }
}

/// Applications that only show every window at once — never-shared ones
/// included, drawn by the system where iyw-claw cannot paint them over — which
/// are never started for an agent: macOS's Mission Control.
pub(super) const SHOWS_EVERY_WINDOW: &[&str] = &["com.apple.exposelauncher"];

/// The most text one `computer_clipboard_write` puts on the clipboard.
pub(super) const MAX_CLIPBOARD_WRITE_CHARS: usize = 100_000;

/// Whether an action copies: a key that copies or cuts, or a menu command
/// named for it — whatever it puts on the clipboard comes from the window,
/// or the application shared as a whole, it was done in.
pub(super) fn copies(request: &ComputerActRequest) -> bool {
    match request {
        ComputerActRequest::Key { chord, .. } | ComputerActRequest::HoldKey { chord, .. } => {
            crate::computer::keys::copies(chord, crate::computer::keys::Platform::current())
        }
        ComputerActRequest::InvokeMenu { path } => path
            .iter()
            .any(|title| crate::computer::keys::names_copy(title)),
        _ => false,
    }
}
