use std::time::Duration;

use super::AgentTurnLeaseRegistry;
use crate::browser::error::BrowserError;
use crate::browser::manager::BrowserSessionManager;
use crate::browser::state::BrowserState;
use crate::browser::types_cdp::BrowserDownloadStatus;

const CLEANUP_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const MAX_CLEANUP_FAILURES: usize = 3;

impl AgentTurnLeaseRegistry {
    async fn begin_cleanup(&self, tab_id: &str) -> bool {
        self.inner
            .lock()
            .await
            .cleanup_running
            .insert(tab_id.to_string())
    }
}

impl BrowserSessionManager {
    pub(super) async fn close_pending_tab(&self, tab_id: &str, reason: &'static str) {
        if !self.agent_turn_leases.begin_cleanup(tab_id).await {
            return;
        }
        let cancellation = self.shutdown_cancellation().await;
        let mut failures = 0;
        loop {
            if cancellation.is_cancelled() {
                break;
            }
            match self.close_unused_pending_tab(tab_id, failures > 0).await {
                Ok(true) => {
                    let mut leases = self.agent_turn_leases.inner.lock().await;
                    if !leases.close_pending.contains(tab_id) {
                        leases.cleanup_running.remove(tab_id);
                        return;
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    failures += 1;
                    tracing::warn!(target: "iyw_claw_browser", browser_tab_id = tab_id,
                        close_reason = reason, failures, error_code = ?error.code, error = %error,
                        "pending browser tab cleanup failed; ownership retained");
                    if failures >= MAX_CLEANUP_FAILURES {
                        break;
                    }
                }
            }
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = tokio::time::sleep(CLEANUP_CHECK_INTERVAL) => {}
            }
        }
        self.agent_turn_leases
            .inner
            .lock()
            .await
            .cleanup_running
            .remove(tab_id);
    }

    async fn close_unused_pending_tab(
        &self,
        tab_id: &str,
        retrying: bool,
    ) -> Result<bool, BrowserError> {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        // 持有租约锁直到逻辑关闭，keep_tab_open/新租约不能插入最终核验与关闭之间。
        let leases = self.agent_turn_leases.inner.lock().await;
        if !leases.close_pending.contains(tab_id) {
            if retrying && !self.state.read().await.tabs.contains_key(tab_id) {
                drop(leases);
                return self.close_browser_tab_locked(tab_id).await.map(|_| true);
            }
            return Ok(true);
        }
        if leases
            .owners
            .get(tab_id)
            .is_some_and(|owners| !owners.is_empty())
        {
            return Ok(false);
        }
        if !self.begin_unused_tab_close(tab_id).await? {
            return Ok(false);
        }
        drop(leases);
        self.close_browser_tab_locked(tab_id).await?;
        self.agent_turn_leases.forget_tab(tab_id).await;
        Ok(true)
    }

    async fn begin_unused_tab_close(&self, tab_id: &str) -> Result<bool, BrowserError> {
        let requests = self.user_action_requests.lock().await;
        if requests
            .values()
            .any(|request| request.snapshot.browser_tab_id == tab_id)
        {
            return Ok(false);
        }
        let gate = self.controls.lock().await.get(tab_id).cloned();
        // CDP 活动记录与逻辑关闭使用同一个状态锁，避免检查后新下载被忽略。
        let mut state = self.state.write().await;
        if pending_tab_has_activity(&state, tab_id) {
            return Ok(false);
        }
        if let Some(gate) = gate {
            if !gate.close_if_idle().await {
                return Ok(false);
            }
        }
        if state.tabs.contains_key(tab_id) {
            state.begin_tab_close(tab_id)?;
        }
        Ok(true)
    }
}

fn pending_tab_has_activity(state: &BrowserState, tab_id: &str) -> bool {
    state
        .tabs
        .get(tab_id)
        .is_some_and(|tab| tab.host_id.is_some())
        || state.claims.values().any(|claim| claim.tab_id == tab_id)
        || state.dialogs.values().any(|dialog| dialog.tab_id == tab_id)
        || state
            .file_choosers
            .values()
            .any(|chooser| chooser.tab_id == tab_id)
        || state.downloads.values().any(|download| {
            download.status == BrowserDownloadStatus::InProgress
                && download.tab_id.as_deref().is_none_or(|id| id == tab_id)
        })
}
