use std::collections::HashSet;

use serde_json::{json, Value};

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::types::{AgentAccess, BrowserAgentIdentity, BrowserStateSnapshot};

pub(super) const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub(super) const SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
pub(super) const MAX_SELECTOR_CHARS: usize = 2_048;
pub(super) const MAX_TEXT_CHARS: usize = 32_768;
pub(super) const MAX_KEY_CHARS: usize = 128;
pub(super) const MAX_WAIT_MS: u64 = 30_000;

impl BrowserSessionManager {
    pub async fn snapshot_for_agent(
        &self,
        identity: &BrowserAgentIdentity,
    ) -> BrowserStateSnapshot {
        let mut snapshot = self.snapshot().await;
        snapshot
            .tabs
            .retain(|tab| tab.agent_access.allows(identity));
        let allowed: HashSet<String> = snapshot
            .tabs
            .iter()
            .map(|tab| tab.browser_tab_id.clone())
            .collect();
        for host in &mut snapshot.hosts {
            host.tab_order.retain(|tab_id| allowed.contains(tab_id));
            if host
                .active_tab_id
                .as_ref()
                .is_some_and(|tab_id| !allowed.contains(tab_id))
            {
                host.active_tab_id = host.tab_order.first().cloned();
            }
        }
        snapshot.hosts.retain(|host| !host.tab_order.is_empty());
        snapshot
            .dialogs
            .retain(|item| allowed.contains(&item.browser_tab_id));
        snapshot
            .file_choosers
            .retain(|item| allowed.contains(&item.browser_tab_id));
        snapshot.downloads.retain(|item| {
            item.browser_tab_id
                .as_ref()
                .is_some_and(|tab_id| allowed.contains(tab_id))
        });
        snapshot
            .view_claims
            .retain(|item| allowed.contains(&item.browser_tab_id));
        snapshot
    }
}

pub(super) fn project_agent_state(
    state: BrowserStateSnapshot,
    active_tab_id: Option<&str>,
    output: Option<Value>,
) -> Value {
    json!({
        "ok": true,
        "runtime": state.runtime,
        "activeTabId": active_tab_id,
        "tabs": state.tabs,
        "dialogs": state.dialogs,
        "downloads": state.downloads,
        "output": output,
    })
}

pub(super) fn agent_access(identity: &BrowserAgentIdentity) -> AgentAccess {
    identity.conversation_id.map_or_else(
        || AgentAccess::PrivateConnection {
            connection_id: identity.connection_id.clone(),
        },
        |conversation_id| AgentAccess::SharedConversation { conversation_id },
    )
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
