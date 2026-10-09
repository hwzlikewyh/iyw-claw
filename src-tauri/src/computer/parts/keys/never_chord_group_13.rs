// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The chords no sharing sends: the ones that lock the screen or log out,
/// and the ones that show every window at once — never-shared ones included,
/// drawn by the system itself where iyw-claw cannot paint them over.
pub(super) fn never_chord(chord: &Chord, platform: Platform) -> bool {
    let Chord { key, modifiers: m } = *chord;
    match platform {
        // Lock Screen (⌃⌘Q), log out (⇧⌘Q, ⌥⇧⌘Q at once); Mission Control
        // (⌃↑) and the application's windows (⌃↓).
        Platform::Mac => {
            (m.meta && key == Key::Char('q') && (m.control || m.shift))
                || (m.control && matches!(key, Key::Up | Key::Down))
        }
        // Lock (Win+L), the secure attention keys (Ctrl+Alt+Delete); Task
        // View (Win+Tab) and the switcher that stays up (Ctrl+Alt+Tab).
        Platform::Windows => {
            (m.meta && matches!(key, Key::Char('l') | Key::Tab))
                || (m.control && m.alt && matches!(key, Key::Delete | Key::Backspace | Key::Tab))
        }
        // Lock (Super+L, Ctrl+Alt+L), log out (Ctrl+Alt+Delete), ending the
        // X server (Ctrl+Alt+Backspace).
        Platform::Linux => {
            (m.meta && key == Key::Char('l'))
                || (m.control
                    && m.alt
                    && matches!(key, Key::Char('l') | Key::Delete | Key::Backspace))
        }
    }
}
