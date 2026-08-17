use std::time::Duration;

use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::state_hosts::{HostExpiry, HostRemoval};
use super::types::BrowserStateSnapshot;

const HOST_WATCH_INTERVAL: Duration = Duration::from_secs(2);

impl BrowserSessionManager {
    pub async fn unregister_browser_host(
        &self,
        host_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let (removed, close_now) = self.remove_browser_host(host_id).await;
        self.release_host_resources(removed, close_now, "host_unregistered")
            .await;
        Ok(self.snapshot().await)
    }

    async fn remove_browser_host(&self, host_id: &str) -> (HostRemoval, Vec<String>) {
        let _tab_guard = self.tab_open_lock.lock().await;
        let tab_ids = self.state.read().await.host_tab_ids(host_id);
        let close_now = self.agent_turn_leases.mark_close_pending(&tab_ids).await;
        let removed = self.state.write().await.unregister_host(host_id);
        (removed, close_now)
    }

    pub async fn unregister_browser_window(&self, window_label: &str) -> BrowserStateSnapshot {
        let host_id = self
            .snapshot()
            .await
            .hosts
            .into_iter()
            .find(|host| host.window_label == window_label)
            .map(|host| host.host_id);
        if let Some(host_id) = host_id {
            let _ = self.unregister_browser_host(&host_id).await;
        }
        self.snapshot().await
    }

    pub(super) fn spawn_host_monitor(&self, host_id: String) {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(HOST_WATCH_INTERVAL).await;
                let removed = manager.expire_browser_host(&host_id).await;
                let Some((removed, close_now)) = removed else {
                    continue;
                };
                manager
                    .release_host_resources(removed, close_now, "host_expired")
                    .await;
                return;
            }
        });
    }

    async fn expire_browser_host(&self, host_id: &str) -> Option<(HostRemoval, Vec<String>)> {
        let _tab_guard = self.tab_open_lock.lock().await;
        let expiry = self.state.write().await.expire_host_if_stale(host_id);
        match expiry {
            HostExpiry::Alive => None,
            HostExpiry::Gone => Some((empty_host_removal(), Vec::new())),
            HostExpiry::Expired(removed) => {
                let close_now = self
                    .agent_turn_leases
                    .mark_close_pending(&removed.tab_ids)
                    .await;
                Some((removed, close_now))
            }
        }
    }

    async fn release_host_resources(
        &self,
        removed: HostRemoval,
        close_now: Vec<String>,
        reason: &'static str,
    ) {
        for tab_id in &removed.tab_ids {
            let _ = self.set_user_held(tab_id, false).await;
            self.streams.close_tab(tab_id).await;
        }
        for claim_id in removed.claim_ids {
            self.streams.close_claim(&claim_id).await;
        }
        tracing::info!(
            target: "iyw_claw_browser",
            close_reason = reason,
            released_tabs = removed.tab_ids.len(),
            immediate_close_tabs = close_now.len(),
            headless_tabs = removed.tab_ids.len().saturating_sub(close_now.len()),
            "browser host resources detached"
        );
        self.spawn_pending_tab_cleanup(close_now, reason);
    }
}

fn empty_host_removal() -> HostRemoval {
    HostRemoval {
        claim_ids: Vec::new(),
        tab_ids: Vec::new(),
    }
}
