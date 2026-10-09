// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Judge `chord` for the entire screen shared: every chord, the desktop's
/// own included, except a paste and the few no sharing reaches
/// ([`never_chord`]).
pub fn classify_for_screen(chord: &Chord, platform: Platform) -> ChordClass {
    if classify(chord, platform) == ChordClass::Paste {
        ChordClass::Paste
    } else if never_chord(chord, platform) {
        ChordClass::Beyond
    } else {
        ChordClass::Window
    }
}
