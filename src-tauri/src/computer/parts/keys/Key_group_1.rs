// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A key on the keyboard, by what it is rather than by any platform's name
/// for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Key {
    /// A letter (`a`–`z`), a digit, or one of the punctuation keys of the
    /// main block. Always lowercase: the key, not the character shift makes
    /// of it.
    Char(char),
    Return,
    Tab,
    Space,
    /// Deletes to the left (the key a Mac labels "delete").
    Backspace,
    /// Deletes to the right (fn+delete on a Mac).
    Delete,
    Escape,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    /// F1–F12.
    F(u8),
}
