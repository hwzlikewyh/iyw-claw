// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A delivery as the record spells it; anything else reads as absent.
pub(super) fn stored_delivery(stored: &str) -> Option<ActDelivery> {
    match stored {
        "background" => Some(ActDelivery::Background),
        "foreground" => Some(ActDelivery::Foreground),
        _ => None,
    }
}

/// A stop shortcut as the record keeps it: off (empty), or a shortcut this
/// platform accepts, written in the one order. Anything else is refused on
/// the way in; on the way out of the database it reads as whatever
/// [`StopShortcut::from_setting`] makes of it, so the record shows the
/// shortcut in force.
pub(super) fn checked_stop_shortcut(spelling: &str) -> Result<String, AppCommandError> {
    if spelling.is_empty() {
        return Ok(String::new());
    }
    StopShortcut::parse(spelling, Platform::current())
        .map(|shortcut| shortcut.to_string())
        .map_err(|e| AppCommandError::configuration_invalid(format!("stop shortcut: {e}")))
}

pub(super) fn stored_stop_shortcut(spelling: &str) -> String {
    StopShortcut::from_setting(spelling, Platform::current())
        .map(|shortcut| shortcut.to_string())
        .unwrap_or_default()
}

/// Trimmed, non-empty, de-duplicated entries in the order they were given.
pub(super) fn normalize_blocklist(entries: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.trim().to_string();
        if !entry.is_empty() && !out.iter().any(|e| e.eq_ignore_ascii_case(&entry)) {
            out.push(entry);
        }
    }
    out
}

/// The keys of default entries taken off the list, once each, in the order
/// given. A key no entry has is dropped: there is nothing it could take off.
pub(super) fn normalize_removed(keys: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        let key = key.trim().to_string();
        if is_default_key(&key) && !out.contains(&key) {
            out.push(key);
        }
    }
    out
}
