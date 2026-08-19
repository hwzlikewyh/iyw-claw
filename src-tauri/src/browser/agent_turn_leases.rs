use std::collections::{HashMap, HashSet};

use tokio::sync::Mutex;

use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::manager::BrowserSessionManager;
use super::types::{BrowserAgentIdentity, BrowserStateSnapshot};

const PENDING_CLOSE_ATTEMPTS: usize = 3;
const PENDING_CLOSE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AgentTurnKey {
    connection_id: String,
    turn_generation: i64,
}

#[derive(Debug, Default)]
struct AgentTurnLeaseState {
    turns: HashMap<AgentTurnKey, HashSet<String>>,
    owners: HashMap<String, HashSet<AgentTurnKey>>,
    close_pending: HashSet<String>,
}

#[derive(Debug, Default)]
pub(super) struct AgentTurnLeaseRegistry {
    inner: Mutex<AgentTurnLeaseState>,
}

impl AgentTurnLeaseRegistry {
    pub async fn register(
        &self,
        identity: &BrowserAgentIdentity,
        tab_id: &str,
    ) -> Result<(), BrowserError> {
        let key = AgentTurnKey::from(identity);
        let mut inner = self.inner.lock().await;
        if inner.close_pending.contains(tab_id)
            && !inner
                .owners
                .get(tab_id)
                .is_some_and(|owners| owners.contains(&key))
        {
            return Err(pending_tab_error(tab_id));
        }
        inner
            .turns
            .entry(key.clone())
            .or_default()
            .insert(tab_id.to_string());
        inner
            .owners
            .entry(tab_id.to_string())
            .or_default()
            .insert(key);
        Ok(())
    }

    pub async fn mark_close_pending(&self, tab_ids: &[String]) -> Vec<String> {
        let mut inner = self.inner.lock().await;
        let mut close_now = Vec::new();
        for tab_id in tab_ids {
            inner.close_pending.insert(tab_id.clone());
            if !inner
                .owners
                .get(tab_id)
                .is_some_and(|owners| !owners.is_empty())
            {
                close_now.push(tab_id.clone());
            }
        }
        close_now
    }

    pub async fn inherit_tab(&self, source_tab_id: &str, target_tab_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let Some(owners) = inner.owners.get(source_tab_id).cloned() else {
            return false;
        };
        for owner in &owners {
            inner
                .turns
                .entry(owner.clone())
                .or_default()
                .insert(target_tab_id.to_string());
        }
        inner
            .owners
            .entry(target_tab_id.to_string())
            .or_default()
            .extend(owners);
        true
    }

    pub async fn ensure_claimable(&self, tab_id: &str) -> Result<(), BrowserError> {
        let inner = self.inner.lock().await;
        if inner.close_pending.contains(tab_id) {
            return Err(pending_tab_error(tab_id));
        }
        Ok(())
    }

    pub async fn release_turn(&self, connection_id: &str, turn_generation: i64) -> Vec<String> {
        let key = AgentTurnKey {
            connection_id: connection_id.to_string(),
            turn_generation,
        };
        let mut inner = self.inner.lock().await;
        release_keys(&mut inner, [key])
    }

    pub async fn release_connection(&self, connection_id: &str) -> Vec<String> {
        let mut inner = self.inner.lock().await;
        let keys = inner
            .turns
            .keys()
            .filter(|key| key.connection_id == connection_id)
            .cloned()
            .collect::<Vec<_>>();
        release_keys(&mut inner, keys)
    }

    pub async fn forget_tab(&self, tab_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.close_pending.remove(tab_id);
        let Some(owners) = inner.owners.remove(tab_id) else {
            return;
        };
        for owner in owners {
            let remove_turn = inner.turns.get_mut(&owner).is_some_and(|tabs| {
                tabs.remove(tab_id);
                tabs.is_empty()
            });
            if remove_turn {
                inner.turns.remove(&owner);
            }
        }
    }

    pub async fn filter_snapshot(
        &self,
        identity: &BrowserAgentIdentity,
        snapshot: &mut BrowserStateSnapshot,
    ) {
        let key = AgentTurnKey::from(identity);
        let inner = self.inner.lock().await;
        let visible = snapshot
            .tabs
            .iter()
            .filter(|tab| tab_visible_to_turn(&inner, &key, &tab.browser_tab_id))
            .map(|tab| tab.browser_tab_id.clone())
            .collect::<HashSet<_>>();
        snapshot
            .tabs
            .retain(|tab| visible.contains(&tab.browser_tab_id));
        snapshot
            .view_claims
            .retain(|claim| visible.contains(&claim.browser_tab_id));
        snapshot
            .dialogs
            .retain(|dialog| visible.contains(&dialog.browser_tab_id));
        snapshot
            .file_choosers
            .retain(|chooser| visible.contains(&chooser.browser_tab_id));
        snapshot.downloads.retain(|download| {
            download
                .browser_tab_id
                .as_ref()
                .is_some_and(|tab_id| visible.contains(tab_id))
        });
    }

    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.turns.is_empty() && inner.close_pending.is_empty()
    }

    pub async fn clear(&self) {
        *self.inner.lock().await = AgentTurnLeaseState::default();
    }
}

impl BrowserSessionManager {
    pub(super) async fn agent_snapshot_for(
        &self,
        identity: &BrowserAgentIdentity,
    ) -> BrowserStateSnapshot {
        let mut snapshot = self.snapshot().await;
        self.agent_turn_leases
            .filter_snapshot(identity, &mut snapshot)
            .await;
        snapshot
    }

    pub(crate) async fn finish_agent_turn(&self, connection_id: &str, turn_generation: i64) {
        let tabs = self
            .agent_turn_leases
            .release_turn(connection_id, turn_generation)
            .await;
        self.spawn_pending_tab_cleanup(tabs, "turn_complete");
    }

    pub(crate) async fn finish_agent_connection(&self, connection_id: &str) {
        let tabs = self
            .agent_turn_leases
            .release_connection(connection_id)
            .await;
        self.spawn_pending_tab_cleanup(tabs, "connection_terminal");
    }

    pub(super) fn spawn_pending_tab_cleanup(&self, tab_ids: Vec<String>, reason: &'static str) {
        let manager = self.clone();
        tokio::spawn(async move {
            for tab_id in tab_ids {
                manager.close_pending_tab(&tab_id, reason).await;
            }
            manager.stop_browser_runtime_if_idle(reason).await;
        });
    }

    pub(super) fn spawn_runtime_idle_check(&self, reason: &'static str) {
        let manager = self.clone();
        tokio::spawn(async move {
            manager.stop_browser_runtime_if_idle(reason).await;
        });
    }

    async fn close_pending_tab(&self, tab_id: &str, reason: &'static str) {
        for attempt in 1..=PENDING_CLOSE_ATTEMPTS {
            match self.close_browser_tab(tab_id).await {
                Ok(_) => return,
                Err(error) => tracing::warn!(
                    target: "iyw_claw_browser",
                    browser_tab_id = tab_id,
                    close_reason = reason,
                    attempt,
                    max_attempts = PENDING_CLOSE_ATTEMPTS,
                    error_code = ?error.code,
                    error = %error,
                    "pending browser tab cleanup failed"
                ),
            }
            if attempt < PENDING_CLOSE_ATTEMPTS {
                tokio::time::sleep(PENDING_CLOSE_RETRY_DELAY).await;
            }
        }
    }
}

impl From<&BrowserAgentIdentity> for AgentTurnKey {
    fn from(identity: &BrowserAgentIdentity) -> Self {
        Self {
            connection_id: identity.connection_id.clone(),
            turn_generation: identity.turn_generation,
        }
    }
}

fn release_keys(
    inner: &mut AgentTurnLeaseState,
    keys: impl IntoIterator<Item = AgentTurnKey>,
) -> Vec<String> {
    let mut closeable = HashSet::new();
    for key in keys {
        let Some(tabs) = inner.turns.remove(&key) else {
            continue;
        };
        for tab_id in tabs {
            let remove_owners = inner.owners.get_mut(&tab_id).is_some_and(|owners| {
                owners.remove(&key);
                owners.is_empty()
            });
            if remove_owners {
                inner.owners.remove(&tab_id);
                if inner.close_pending.contains(&tab_id) {
                    closeable.insert(tab_id);
                }
            }
        }
    }
    closeable.into_iter().collect()
}

fn tab_visible_to_turn(inner: &AgentTurnLeaseState, key: &AgentTurnKey, tab_id: &str) -> bool {
    !inner.close_pending.contains(tab_id)
        || inner
            .owners
            .get(tab_id)
            .is_some_and(|owners| owners.contains(key))
}

fn pending_tab_error(tab_id: &str) -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserTabGone,
        "The browser tab was closed by the user",
    )
    .with_context(BrowserErrorContext {
        browser_tab_id: Some(tab_id.to_string()),
        ..BrowserErrorContext::default()
    })
}
