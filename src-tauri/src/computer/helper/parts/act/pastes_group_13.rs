// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether `action` pastes by its key: ⌘V / Ctrl+V and the rest of its
/// kind (`keys::classify`).
pub fn pastes(action: &WindowAction) -> bool {
    match action {
        WindowAction::Key { chord, .. } => {
            crate::computer::keys::classify(chord, Platform::current())
                == crate::computer::keys::ChordClass::Paste
        }
        // A menu command named for pasting, on every platform; the one
        // whose shortcut is ⌘V is caught on macOS by `menu_in_reach`.
        WindowAction::InvokeMenu { path } => path
            .iter()
            .any(|title| crate::computer::keys::names_paste(title)),
        _ => false,
    }
}

/// A paste refused: the clipboard is not what the agent put there.
pub fn paste_refused() -> HelperError {
    HelperError::new(
        HelperErrorCode::PasteRefused,
        "That pastes, and the clipboard holds what the user put there, not what you copied from \
         a window you may read or wrote with computer_clipboard_write. Type the text with \
         computer_type instead, or copy it from a shared window first.",
    )
}
