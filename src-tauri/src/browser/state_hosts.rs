use std::time::{Duration, Instant};

use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::records::HostRecord;
use super::state::BrowserState;
use super::types::{BrowserHostKind, BrowserViewStatus};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) enum HostExpiry {
    Alive,
    Gone,
    Expired(HostRemoval),
}

pub(super) struct HostRemoval {
    pub claim_ids: Vec<String>,
    pub tab_ids: Vec<String>,
}

impl BrowserState {
    pub fn register_host(
        &mut self,
        window_label: String,
        kind: BrowserHostKind,
    ) -> Result<(String, u64, bool), BrowserError> {
        if let Some(host) = self
            .hosts
            .values_mut()
            .find(|host| host.window_label == window_label)
        {
            if host.kind != kind {
                return Err(view_conflict("The browser window host kind changed"));
            }
            host.visible = true;
            host.last_heartbeat = Instant::now();
            return Ok((host.id.clone(), host.generation, false));
        }
        ensure_host_capacity(&self.hosts, kind)?;
        let host_id = Uuid::new_v4().to_string();
        self.hosts.insert(
            host_id.clone(),
            HostRecord {
                id: host_id.clone(),
                window_label,
                kind,
                generation: 1,
                visible: true,
                tab_order: Vec::new(),
                active_tab_id: None,
                last_heartbeat: Instant::now(),
            },
        );
        Ok((host_id, 1, true))
    }

    pub fn heartbeat_host(
        &mut self,
        host_id: &str,
        generation: u64,
        visible: bool,
    ) -> Result<bool, BrowserError> {
        let host = self.hosts.get_mut(host_id).ok_or_else(host_gone)?;
        if host.generation != generation {
            return Err(stale_host(host_id, generation));
        }
        let became_hidden = host.visible && !visible;
        host.visible = visible;
        host.last_heartbeat = Instant::now();
        Ok(became_hidden)
    }

    pub fn expire_host_if_stale(&mut self, host_id: &str) -> HostExpiry {
        let Some(host) = self.hosts.get(host_id) else {
            return HostExpiry::Gone;
        };
        if host.last_heartbeat.elapsed() < HOST_TIMEOUT {
            return HostExpiry::Alive;
        }
        HostExpiry::Expired(self.unregister_host(host_id))
    }

    pub fn unregister_host(&mut self, host_id: &str) -> HostRemoval {
        let claims: Vec<String> = self
            .claims
            .values()
            .filter(|claim| {
                claim.source_host_id.as_deref() == Some(host_id) || claim.target_host_id == host_id
            })
            .map(|claim| claim.id.clone())
            .collect();
        let tab_ids: Vec<String> = self
            .tabs
            .values()
            .filter(|tab| tab.host_id.as_deref() == Some(host_id))
            .map(|tab| tab.id.clone())
            .collect();
        for claim_id in &claims {
            self.abort_view_claim_unchecked(claim_id);
        }
        if self.hosts.remove(host_id).is_none() {
            return HostRemoval {
                claim_ids: claims,
                tab_ids,
            };
        }
        for tab in self
            .tabs
            .values_mut()
            .filter(|tab| tab.host_id.as_deref() == Some(host_id))
        {
            tab.host_id = None;
            tab.view_status = BrowserViewStatus::Unclaimed;
            tab.view_generation = tab.view_generation.saturating_add(1);
        }
        HostRemoval {
            claim_ids: claims,
            tab_ids,
        }
    }

    pub fn set_host_visible(
        &mut self,
        host_id: &str,
        generation: u64,
        visible: bool,
    ) -> Result<bool, BrowserError> {
        let host = self.hosts.get_mut(host_id).ok_or_else(host_gone)?;
        if host.generation != generation {
            return Err(stale_host(host_id, generation));
        }
        let became_hidden = host.visible && !visible;
        host.visible = visible;
        Ok(became_hidden)
    }

    pub fn activate_host_tab(
        &mut self,
        host_id: &str,
        generation: u64,
        tab_id: &str,
    ) -> Result<Option<String>, BrowserError> {
        let host = self.hosts.get_mut(host_id).ok_or_else(host_gone)?;
        if host.generation != generation {
            return Err(stale_host(host_id, generation));
        }
        if !host.tab_order.iter().any(|id| id == tab_id) {
            return Err(view_conflict("The browser host does not own this tab"));
        }
        Ok(host.active_tab_id.replace(tab_id.to_string()))
    }

    pub fn hidden_host_tabs(&self, host_id: &str, generation: u64) -> Option<Vec<String>> {
        let host = self.hosts.get(host_id)?;
        if host.generation != generation || host.visible {
            return None;
        }
        Some(host.tab_order.clone())
    }
}

fn ensure_host_capacity(
    hosts: &std::collections::BTreeMap<String, HostRecord>,
    kind: BrowserHostKind,
) -> Result<(), BrowserError> {
    let detached = hosts
        .values()
        .filter(|host| host.kind == BrowserHostKind::Detached)
        .count();
    if kind == BrowserHostKind::Detached && detached >= super::MAX_DETACHED_BROWSER_WINDOWS {
        return Err(view_conflict(
            "The detached browser window limit has been reached",
        ));
    }
    Ok(())
}

fn stale_host(host_id: &str, generation: u64) -> BrowserError {
    BrowserError::stale_generation(BrowserErrorContext {
        operation_id: Some(host_id.to_string()),
        view_generation: Some(generation),
        ..BrowserErrorContext::default()
    })
}

pub(super) fn host_gone() -> BrowserError {
    view_conflict("The browser window host no longer exists")
}

pub(super) fn view_conflict(message: &str) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserViewConflict, message)
}
