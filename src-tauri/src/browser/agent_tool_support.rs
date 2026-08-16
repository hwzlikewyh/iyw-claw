use serde_json::{json, Value};

use super::error::{BrowserError, BrowserErrorCode};
use super::types::{BrowserHostKind, BrowserStateSnapshot};

pub(super) const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub(super) const SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
pub(super) const MAX_SELECTOR_CHARS: usize = 2_048;
pub(super) const MAX_TEXT_CHARS: usize = 32_768;
pub(super) const MAX_KEY_CHARS: usize = 128;
pub(super) const MAX_WAIT_MS: u64 = 30_000;

pub(super) fn project_agent_state(
    state: BrowserStateSnapshot,
    target_tab_id: Option<&str>,
    output: Option<Value>,
) -> Value {
    let active_tab_id = default_agent_tab_id(&state);
    json!({
        "ok": true,
        "runtime": state.runtime,
        "activeTabId": active_tab_id,
        "targetTabId": target_tab_id,
        "tabs": state.tabs,
        "dialogs": state.dialogs,
        "downloads": state.downloads,
        "output": output,
    })
}

pub(super) fn default_agent_tab_id(state: &BrowserStateSnapshot) -> Option<String> {
    [BrowserHostKind::Docked, BrowserHostKind::Detached]
        .into_iter()
        .find_map(|kind| active_host_tab(state, kind))
        .or_else(|| state.tabs.first().map(|tab| tab.browser_tab_id.clone()))
}

pub(super) fn preferred_agent_host_id(state: &BrowserStateSnapshot) -> Option<String> {
    state
        .hosts
        .iter()
        .find(|host| host.kind == BrowserHostKind::Docked && host.visible)
        .map(|host| host.host_id.clone())
}

fn active_host_tab(state: &BrowserStateSnapshot, kind: BrowserHostKind) -> Option<String> {
    state
        .hosts
        .iter()
        .filter(|host| host.kind == kind && host.visible)
        .find_map(|host| {
            let tab_id = host.active_tab_id.as_ref()?;
            state
                .tabs
                .iter()
                .any(|tab| tab.browser_tab_id == *tab_id)
                .then(|| tab_id.clone())
        })
}

pub(super) fn required_string<'a>(
    input: &'a Value,
    field: &str,
    max_chars: usize,
) -> Result<&'a str, BrowserError> {
    optional_string(input, field, max_chars)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_argument(&format!("Missing browser argument: {field}")))
}

pub(super) fn optional_string<'a>(
    input: &'a Value,
    field: &str,
    max_chars: usize,
) -> Result<Option<&'a str>, BrowserError> {
    let Some(value) = input.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .filter(|text| text.chars().count() <= max_chars && !text.contains('\0'))
        .ok_or_else(|| invalid_argument(&format!("Invalid browser argument: {field}")))?;
    Ok(Some(value))
}

pub(super) fn optional_bool(input: &Value, field: &str) -> Result<Option<bool>, BrowserError> {
    let Some(value) = input.get(field) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| invalid_argument(&format!("Invalid browser argument: {field}")))
}

pub(super) fn invalid_argument(message: &str) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserInvalidArgument, message)
}

pub(super) fn browser_error(error: BrowserError) -> Value {
    json!({
        "error": {
            "code": error.code,
            "message": error.message,
            "retryable": error.retryable,
            "effectMayHaveOccurred": error.effect_may_have_occurred,
            "context": error.context,
        }
    })
}
