// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The desktop's own shortcuts on `platform`, which no application grant
/// reaches: they switch applications, open the launcher, take screenshots,
/// lock the screen or log out, force applications to quit, or move between
/// desktops.
pub(super) fn desktop_chord(chord: &Chord, platform: Platform) -> bool {
    let Chord { key, modifiers: m } = *chord;
    match platform {
        Platform::Mac => {
            let (cmd, ctrl, opt, shift) = (m.meta, m.control, m.alt, m.shift);
            // ⌘Tab; Spotlight (⌘Space, ⌥⌘Space), the input sources (⌃Space)
            // and the character viewer (⌃⌘Space).
            (cmd && key == Key::Tab)
                || (key == Key::Space && (cmd || ctrl))
                // Screenshots: ⇧⌘3 to ⇧⌘6, with ⌃ to the clipboard.
                || (cmd && shift && matches!(key, Key::Char('3' | '4' | '5' | '6')))
                // Lock the screen (⌃⌘Q), log out (⇧⌘Q), Force Quit (⌥⌘Esc).
                || (cmd && ctrl && key == Key::Char('q'))
                || (cmd && shift && key == Key::Char('q'))
                || (cmd && opt && key == Key::Escape)
                // The Dock (⌥⌘D), hiding every other application (⌥⌘H).
                || (cmd && opt && matches!(key, Key::Char('d' | 'h')))
                // Mission Control and the desktops (⌃ and an arrow), keyboard
                // access to the menu bar, the Dock and the rest (⌃F1–F12),
                // VoiceOver (⌘F5), Show Desktop (F11).
                || (ctrl && (key.is_arrow() || matches!(key, Key::F(_))))
                || (cmd && key == Key::F(5))
                || (!cmd && !ctrl && !opt && key == Key::F(11))
        }
        // The Windows key; switching (Alt+Tab, Alt+Esc); Start (Ctrl+Esc) and
        // the Task Manager (Ctrl+Shift+Esc); Ctrl+Alt with anything — the
        // secure attention keys, the display's rotation, and AltGr's
        // characters, which are typed with computer_type.
        Platform::Windows => {
            m.meta
                || (m.alt && matches!(key, Key::Tab | Key::Escape))
                || (m.control && key == Key::Escape)
                || (m.control && m.alt)
        }
        // Super; switching (Alt+Tab, Alt+`, Alt+Esc); the launcher or the
        // process monitor (Ctrl+Esc); the window manager's Alt+F-keys (the
        // activities, the run dialog, moving and resizing) but Alt+F4, which
        // closes the application's own window; Ctrl+Alt with anything — a
        // terminal, the lock screen, logging out, the workspaces, the text
        // consoles.
        Platform::Linux => {
            m.meta
                || (m.alt && matches!(key, Key::Tab | Key::Escape | Key::Char('`')))
                || (m.control && key == Key::Escape)
                || (m.alt && matches!(key, Key::F(n) if n != 4))
                || (m.control && m.alt)
        }
    }
}
