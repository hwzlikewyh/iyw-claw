//! The shortcut that stops every agent at once, from anywhere on the desktop.
//!
//! It is registered with the OS as a global hotkey (see
//! `commands::computer`), and that is why the keys are a closed list. On
//! macOS a hotkey on an ordinary key is a Carbon hotkey, which needs no
//! permission at all; a hotkey on a media key is watched through an event
//! tap, which needs Input Monitoring — a permission iyw-claw must never hold,
//! because every agent's shell would inherit it. So only the keys below are
//! ever registered, whatever a settings record says.
//!
//! It must also be hard to press by accident and hard to take from another
//! application: two modifiers at least, one of them Control — or, on a Mac,
//! Command. (macOS no longer honours a hotkey held with Option alone.)
//!
//! Spelled as modifiers then one key, joined by `+`, in a fixed order:
//! `Control+Alt+Shift+Command+<code>`, the key named by its W3C `code` — the
//! physical key, whatever the keyboard layout prints on it. An empty string
//! is "no shortcut".

use std::fmt;

use serde::Serialize;

use crate::computer::keys::Platform;

/// Where the stop shortcut stands. Held with the OS by the desktop app
/// alone: iyw-claw-server has none, and says so with this left empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopKeyStatus {
    /// The shortcut in force, spelled as the settings spell it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// The shortcut the settings name that the OS would not take — most
    /// likely another application holds it — and what the OS said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The punctuation keys of the main block, by W3C code.
const PUNCTUATION: &[&str] = &[
    "Minus",
    "Equal",
    "BracketLeft",
    "BracketRight",
    "Backslash",
    "Semicolon",
    "Quote",
    "Backquote",
    "Comma",
    "Period",
    "Slash",
];

/// The modifiers held with the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ShortcutModifiers {
    pub control: bool,
    /// Option on a Mac.
    pub alt: bool,
    pub shift: bool,
    /// Macs only.
    pub command: bool,
}

impl ShortcutModifiers {
    fn count(&self) -> usize {
        [self.control, self.alt, self.shift, self.command]
            .into_iter()
            .filter(|held| *held)
            .count()
    }
}

/// A shortcut that may stop every agent: see the module note.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StopShortcut {
    pub modifiers: ShortcutModifiers,
    /// The key's W3C `code`, one of [`is_allowed_key`]'s.
    pub code: String,
}

/// Why a spelling is not a stop shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutError {
    /// Not modifiers and a key joined by `+`, or a modifier named twice.
    Malformed,
    /// A key this shortcut cannot be on.
    UnsupportedKey,
    /// Fewer than two modifiers, or neither Control nor (on a Mac) Command.
    WeakModifiers,
    /// Command, off a Mac.
    CommandOffMac,
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ShortcutError::Malformed => "not a shortcut: modifiers and one key, joined by `+`",
            ShortcutError::UnsupportedKey => {
                "the stop shortcut is on a letter, a digit, F1–F12, Escape or a punctuation key"
            }
            ShortcutError::WeakModifiers => {
                "the stop shortcut holds two modifiers or more, one of them Control (or Command \
                 on a Mac)"
            }
            ShortcutError::CommandOffMac => "Command is a Mac modifier",
        })
    }
}

/// Whether the stop shortcut can be on `code`: a letter, a digit, F1–F12,
/// Escape, or a punctuation key of the main block.
pub fn is_allowed_key(code: &str) -> bool {
    let one = |rest: &str, class: fn(&char) -> bool| {
        let mut chars = rest.chars();
        matches!((chars.next(), chars.next()), (Some(c), None) if class(&c))
    };
    if let Some(letter) = code.strip_prefix("Key") {
        return one(letter, char::is_ascii_uppercase);
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return one(digit, char::is_ascii_digit);
    }
    if let Some(n) = code.strip_prefix('F') {
        return !n.starts_with('0') && n.parse::<u8>().is_ok_and(|n| (1..=12).contains(&n));
    }
    code == "Escape" || PUNCTUATION.contains(&code)
}

impl StopShortcut {
    /// Read a spelling (see the module note) as a stop shortcut on
    /// `platform`. Modifiers may come in any order; the key comes last.
    pub fn parse(spelling: &str, platform: Platform) -> Result<StopShortcut, ShortcutError> {
        let mut parts: Vec<&str> = spelling.split('+').collect();
        let code = parts
            .pop()
            .filter(|c| !c.is_empty())
            .ok_or(ShortcutError::Malformed)?;
        let mut modifiers = ShortcutModifiers::default();
        for part in parts {
            let held = match part {
                "Control" => &mut modifiers.control,
                "Alt" => &mut modifiers.alt,
                "Shift" => &mut modifiers.shift,
                "Command" => &mut modifiers.command,
                _ => return Err(ShortcutError::Malformed),
            };
            if *held {
                return Err(ShortcutError::Malformed);
            }
            *held = true;
        }
        if !is_allowed_key(code) {
            return Err(ShortcutError::UnsupportedKey);
        }
        let mac = platform == Platform::Mac;
        if modifiers.command && !mac {
            return Err(ShortcutError::CommandOffMac);
        }
        if modifiers.count() < 2 || !(modifiers.control || modifiers.command) {
            return Err(ShortcutError::WeakModifiers);
        }
        Ok(StopShortcut {
            modifiers,
            code: code.to_string(),
        })
    }

    /// The shortcut a person gets until they choose another: ⌃⌘⎋ on a Mac,
    /// Ctrl+Alt+Esc elsewhere.
    pub fn default_for(platform: Platform) -> StopShortcut {
        StopShortcut {
            modifiers: ShortcutModifiers {
                control: true,
                alt: platform != Platform::Mac,
                shift: false,
                command: platform == Platform::Mac,
            },
            code: "Escape".to_string(),
        }
    }

    /// What a settings record's spelling comes to on `platform`: nothing for
    /// the empty string, the shortcut it spells, or — for a spelling that is
    /// not one here (a record from another platform's iyw-claw) — the default,
    /// because a stop shortcut that silently stopped existing is the worse
    /// surprise.
    pub fn from_setting(spelling: &str, platform: Platform) -> Option<StopShortcut> {
        if spelling.is_empty() {
            return None;
        }
        Some(Self::parse(spelling, platform).unwrap_or_else(|_| Self::default_for(platform)))
    }
}

#[cfg(feature = "tauri-runtime")]
impl StopShortcut {
    /// The hotkey to register with the OS. `None` only for a key outside the
    /// list, which [`StopShortcut::parse`] never lets through.
    pub fn hotkey(&self) -> Option<tauri_plugin_global_shortcut::Shortcut> {
        use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
        if !is_allowed_key(&self.code) {
            return None;
        }
        let code: Code = self.code.parse().ok()?;
        let m = &self.modifiers;
        let mut modifiers = Modifiers::empty();
        for (held, flag) in [
            (m.control, Modifiers::CONTROL),
            (m.alt, Modifiers::ALT),
            (m.shift, Modifiers::SHIFT),
            (m.command, Modifiers::SUPER),
        ] {
            if held {
                modifiers |= flag;
            }
        }
        Some(Shortcut::new(Some(modifiers), code))
    }
}

impl fmt::Display for StopShortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = &self.modifiers;
        for (held, name) in [
            (m.control, "Control"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.command, "Command"),
        ] {
            if held {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.code)
    }
}
