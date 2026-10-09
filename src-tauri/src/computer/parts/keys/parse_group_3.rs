// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Key {
    /// Parse an agent's name for a key. Case does not matter; a few common
    /// aliases are read (`enter`, `esc`, `del`, `arrowup`, …).
    pub fn parse(name: &str) -> Result<Key, String> {
        let raw = name.trim();
        if raw.chars().count() == 1 {
            let c = raw.chars().next().unwrap_or(' ');
            let lower = c.to_ascii_lowercase();
            return if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
                Ok(Key::Char(lower))
            } else if PUNCTUATION.contains(&c) {
                Ok(Key::Char(c))
            } else if c == ' ' {
                Ok(Key::Space)
            } else {
                Err(format!(
                    "`{raw}` is not a key this tool presses; type text with computer_type"
                ))
            };
        }
        let lower = raw.to_ascii_lowercase().replace(['-', ' '], "_");
        let key = match lower.as_str() {
            "return" | "enter" => Key::Return,
            "tab" => Key::Tab,
            "space" | "spacebar" => Key::Space,
            "backspace" => Key::Backspace,
            "delete" | "del" | "forward_delete" | "forwarddelete" => Key::Delete,
            "escape" | "esc" => Key::Escape,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" | "page_up" | "pgup" => Key::PageUp,
            "pagedown" | "page_down" | "pgdn" => Key::PageDown,
            "up" | "arrowup" | "up_arrow" | "arrow_up" => Key::Up,
            "down" | "arrowdown" | "down_arrow" | "arrow_down" => Key::Down,
            "left" | "arrowleft" | "left_arrow" | "arrow_left" => Key::Left,
            "right" | "arrowright" | "right_arrow" | "arrow_right" => Key::Right,
            other => match other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                Some(n) if (1..=12).contains(&n) => Key::F(n),
                _ => {
                    return Err(format!(
                        "`{raw}` is not a key this tool knows. Keys: a letter, a digit or a \
                         punctuation key, return, tab, space, backspace, delete, escape, home, \
                         end, pageup, pagedown, up, down, left, right, f1–f12"
                    ))
                }
            },
        };
        Ok(key)
    }

    /// Whether pressing it (with nothing held, or shift) puts a character
    /// into whatever has focus.
    pub fn is_character(self) -> bool {
        matches!(self, Key::Char(_) | Key::Space)
    }

    /// The key's name as `platform`'s driver reads it.
    pub fn driver_name(self, platform: Platform) -> String {
        match self {
            Key::Char(c) => c.to_string(),
            Key::Return => "return".into(),
            Key::Tab => "tab".into(),
            Key::Space => "space".into(),
            Key::Backspace => "backspace".into(),
            // The Mac driver's "delete" is the Mac key of that name, which
            // deletes to the left.
            Key::Delete => match platform {
                Platform::Mac => "forward_delete".into(),
                Platform::Windows | Platform::Linux => "delete".into(),
            },
            Key::Escape => "escape".into(),
            Key::Home => "home".into(),
            Key::End => "end".into(),
            Key::PageUp => "pageup".into(),
            Key::PageDown => "pagedown".into(),
            Key::Up => "up".into(),
            Key::Down => "down".into(),
            Key::Left => "left".into(),
            Key::Right => "right".into(),
            Key::F(n) => format!("f{n}"),
        }
    }

    pub(in crate::computer::keys) fn is_arrow(self) -> bool {
        matches!(self, Key::Up | Key::Down | Key::Left | Key::Right)
    }
}
