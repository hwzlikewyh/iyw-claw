// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether a menu command or a control is named for pasting — which writes
/// the person's clipboard into a window, as ⌘V / Ctrl+V does (see the module
/// note).
pub fn names_paste(title: &str) -> bool {
    let title = title.to_lowercase();
    PASTE_WORDS.iter().any(|word| title.contains(word))
}

/// Whether a window grant reaches `modifiers` held during a click or a drag
/// on `platform`: Shift and Control everywhere — a click with them stays the
/// window's own (extending a selection, a context click). On a Mac Command
/// too, and not Option: Option-clicking a window's close button closes every
/// window of the application, and minimizing or zooming acts on them all the
/// same. Elsewhere Alt, and never the Windows / Super key, which is the
/// desktop's.
pub fn pointer_modifiers_allowed(modifiers: Modifiers, platform: Platform) -> bool {
    match platform {
        Platform::Mac => !modifiers.alt,
        Platform::Windows | Platform::Linux => !modifiers.meta,
    }
}

/// Whether `platform`'s driver holds `modifiers` down over a drag. macOS's
/// does. Windows' and Linux's drag without them and answer that they
/// dragged — a move where a copy was meant — so there a drag with keys held
/// is not sent at all.
pub fn drag_carries_modifiers(modifiers: Modifiers, platform: Platform) -> bool {
    modifiers.is_empty() || platform == Platform::Mac
}

/// The chords a window grant allows, in words, for a refusal to quote.
pub fn window_chords_note(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => {
            "On a shared window you may press the editing and navigation keys (return, tab, \
             escape, backspace, delete, the arrows, home, end, page up/down — with or without \
             shift, except shift+delete) and ⌘A, ⌘C, ⌘X, ⌘Z, ⇧⌘Z, ⌘F, ⌘G, ⇧⌘G, ⌘ or ⌥ with \
             an arrow (with or without shift), ⌥ with backspace or delete."
        }
        Platform::Windows | Platform::Linux => {
            "On a shared window you may press the editing and navigation keys (enter, tab, \
             escape, backspace, delete, the arrows, home, end, page up/down — with or without \
             shift, except shift+delete) and Ctrl+A, Ctrl+C, Ctrl+X, Ctrl+Z, Ctrl+Shift+Z, \
             Ctrl+Y, Ctrl+F, Ctrl+G, Ctrl with an arrow, home or end (with or without shift), \
             Ctrl with backspace or delete."
        }
    }
}
