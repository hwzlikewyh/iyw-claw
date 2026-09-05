use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::Value;

use super::agent_browser::{BrowserRoute, BrowserRouteProvider};
use super::agent_tool_support::invalid_argument;
use super::error::BrowserError;
use super::types::BrowserAgentIdentity;

pub(super) fn route_key(identity: &BrowserAgentIdentity, input: &Value) -> String {
    if let Some(task_id) = input.get("task_id").and_then(Value::as_str) {
        return format!("task:{}:{task_id}", identity.connection_id);
    }
    format!(
        "conversation:{}:{}",
        identity.connection_id,
        identity.conversation_id.unwrap_or_default()
    )
}

pub(super) fn session_name(identity: &BrowserAgentIdentity) -> String {
    let mut hasher = DefaultHasher::new();
    identity.connection_id.hash(&mut hasher);
    identity.conversation_id.hash(&mut hasher);
    format!("iyw-{:x}", hasher.finish())
}

pub(super) fn input_requests_managed(input: &Value) -> bool {
    requested_tab_id(input).is_some_and(|tab_id| !tab_id.starts_with("opencli:"))
}

pub(super) fn opencli_route_from_input(
    identity: &BrowserAgentIdentity,
    input: &Value,
) -> Option<BrowserRoute> {
    let tab_id = requested_tab_id(input)?.strip_prefix("opencli:")?;
    let target = tab_id
        .split_once(':')
        .map(|(_, target)| target.to_string())
        .filter(|target| !target.is_empty());
    Some(BrowserRoute {
        provider: BrowserRouteProvider::Opencli {
            session: session_name(identity),
            target,
        },
    })
}

pub(super) fn ensure_provider_matches_input(
    route: Option<&BrowserRoute>,
    input: &Value,
) -> Result<(), BrowserError> {
    let Some(tab_id) = requested_tab_id(input) else {
        return Ok(());
    };
    let requested_opencli = tab_id.starts_with("opencli:");
    let mismatch = match route.map(|route| &route.provider) {
        Some(BrowserRouteProvider::Opencli { .. }) => !requested_opencli,
        Some(BrowserRouteProvider::Managed { .. }) => requested_opencli,
        None => false,
    };
    if mismatch {
        return Err(invalid_argument(
            "The supplied tab_id belongs to a different browser provider than this task",
        ));
    }
    Ok(())
}

pub(super) fn validate_opencli_tab_session(
    identity: &BrowserAgentIdentity,
    input: &Value,
) -> Result<(), BrowserError> {
    let Some(tab_id) = requested_tab_id(input) else {
        return Ok(());
    };
    let Some(value) = tab_id.strip_prefix("opencli:") else {
        return Ok(());
    };
    let supplied_session = value.split_once(':').map_or(value, |(session, _)| session);
    if supplied_session != session_name(identity) {
        return Err(invalid_argument(
            "The supplied OpenCLI tab_id belongs to a different Agent session",
        ));
    }
    Ok(())
}

fn requested_tab_id(input: &Value) -> Option<&str> {
    input
        .get("tab_id")
        .or_else(|| input.get("tabId"))
        .and_then(Value::as_str)
}
