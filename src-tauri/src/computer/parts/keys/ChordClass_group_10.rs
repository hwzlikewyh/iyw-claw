// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What a window grant makes of a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordClass {
    /// Stays inside the window: editing and navigation.
    Window,
    /// Writes the clipboard into the window. See the module note.
    Paste,
    /// Acts on the application or the desktop — or is simply not on the
    /// list. Needs more than a window grant.
    Beyond,
}

/// Judge `chord` for a window grant on `platform`. See the module note.
pub fn classify(chord: &Chord, platform: Platform) -> ChordClass {
    let Chord { key, modifiers: m } = *chord;
    // The platform's shortcut modifier, and the one that is not it. On
    // Windows and Linux the Windows / Super key is the desktop's own and
    // never reaches a single window.
    let (primary, other) = match platform {
        Platform::Mac => (m.meta, m.control),
        Platform::Windows | Platform::Linux => (m.control, m.meta),
    };
    if other {
        return ChordClass::Beyond;
    }
    if primary && key == Key::Char('v') {
        return ChordClass::Paste;
    }
    match (primary, m.alt) {
        // Nothing held but perhaps shift: every key but the function keys,
        // which applications (and some desktops) bind to their own commands,
        // and shift+delete, which deletes a selected file for good in the
        // Windows File Explorer.
        (false, false) => {
            if matches!(key, Key::F(_)) || (m.shift && key == Key::Delete) {
                ChordClass::Beyond
            } else {
                ChordClass::Window
            }
        }
        // The shortcut modifier: the editing chords, and moving through text.
        // Shift only where it is the same command backwards (redo, find
        // previous) or extends a selection — ⇧⌘A opens a folder in the
        // Finder, and ⇧⌘⌫ empties the Trash. ⌘ with backspace or delete is a
        // menu command on a Mac (the Finder's Move to Trash), so only Ctrl
        // deletes by word elsewhere.
        (true, false) => {
            let editing = match key {
                Key::Char('a' | 'c' | 'x' | 'f') => !m.shift,
                Key::Char('z' | 'g') => true,
                Key::Char('y') => platform != Platform::Mac && !m.shift,
                _ => false,
            };
            let moving = key.is_arrow()
                || (platform != Platform::Mac
                    && (matches!(key, Key::Home | Key::End)
                        || (matches!(key, Key::Backspace | Key::Delete) && !m.shift)));
            if editing || moving {
                ChordClass::Window
            } else {
                ChordClass::Beyond
            }
        }
        // Option on a Mac moves and deletes by word; Alt elsewhere opens the
        // application's menus.
        (false, true) => {
            let by_word =
                key.is_arrow() || (matches!(key, Key::Backspace | Key::Delete) && !m.shift);
            if platform == Platform::Mac && by_word {
                ChordClass::Window
            } else {
                ChordClass::Beyond
            }
        }
        (true, true) => ChordClass::Beyond,
    }
}

/// Judge `chord` for a grant on a whole application: every chord the
/// application takes, except a paste (see the module note) and the
/// desktop's own shortcuts ([`desktop_chord`]), which stay [`ChordClass::Beyond`].
pub fn classify_for_app(chord: &Chord, platform: Platform) -> ChordClass {
    if classify(chord, platform) == ChordClass::Paste {
        ChordClass::Paste
    } else if desktop_chord(chord, platform) {
        ChordClass::Beyond
    } else {
        ChordClass::Window
    }
}
