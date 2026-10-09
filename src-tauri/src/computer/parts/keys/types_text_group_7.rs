// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Chord {
    /// Whether the chord types a character into whatever has focus: a
    /// character key with nothing held but, perhaps, shift. Such a key goes
    /// only into an element the agent names, and never into a secret one.
    pub fn types_text(&self) -> bool {
        let m = self.modifiers;
        self.key.is_character() && !m.control && !m.alt && !m.meta
    }
}
