use std::collections::HashSet;
use std::time::Duration;

use super::ConnectionManager;
use crate::acp::error::AcpError;
use crate::acp::resource_governor::reclaim_block_reason;

const MAX_SPECULATIVE_RUNTIMES: usize = 2;

impl ConnectionManager {
    pub(super) async fn speculative_runtime_capacity(&self) -> Result<usize, AcpError> {
        let limit = match self.version_center_db.get() {
            Some(db) => crate::commands::idle_agent_settings::get_idle_agent_settings_core(db)
                .await
                .map_err(|error| AcpError::protocol(error.message))?
                .max_idle_connections
                .unwrap_or(MAX_SPECULATIVE_RUNTIMES),
            None => MAX_SPECULATIVE_RUNTIMES,
        };
        let prepared = self.unclaimed_preparation_ids().await;
        let (idle, _) = self.recoverable_idle_runtime_count(&prepared).await;
        let warm = self.runtime_hosts.unused_runtime_count().await;
        Ok(limit.saturating_sub(prepared.len() + idle + warm))
    }

    pub(super) async fn remaining_idle_connection_budget(
        &self,
        limit: Option<usize>,
    ) -> Option<usize> {
        let limit = limit?;
        let _admission = self.speculative_runtime_gate.lock().await;
        let prepared = self.unclaimed_preparation_ids().await;
        let (idle, idle_sessions) = self.recoverable_idle_runtime_count(&prepared).await;
        self.runtime_hosts
            .retire_unused_excess(limit.saturating_sub(idle + prepared.len()))
            .await;
        let warm = self.runtime_hosts.unused_runtime_count().await;
        let mut excess = (idle + warm + prepared.len()).saturating_sub(limit);
        let pool = self.prepared_sessions.lock().await;
        for entry in pool.entries.values().filter(|entry| !entry.is_claimed()) {
            if excess == 0 {
                break;
            }
            entry.cancel.cancel();
            excess -= 1;
        }
        // 正在清理的实例仍占名额，直到生命周期确认退出。
        Some(limit.saturating_sub(warm + prepared.len()) + idle_sessions.saturating_sub(idle))
    }

    async fn unclaimed_preparation_ids(&self) -> HashSet<String> {
        self.prepared_sessions
            .lock()
            .await
            .entries
            .values()
            .filter(|entry| !entry.is_claimed())
            .map(|entry| entry.id.clone())
            .collect()
    }

    async fn recoverable_idle_runtime_count(&self, prepared: &HashSet<String>) -> (usize, usize) {
        let states = self
            .connections
            .lock()
            .await
            .iter()
            .filter(|(id, _)| !prepared.contains(*id))
            .map(|(_, connection)| connection.state.clone())
            .collect::<Vec<_>>();
        let mut pids = HashSet::new();
        let mut unknown = 0;
        let mut sessions = 0;
        for state in states {
            let Ok(state) = state.try_read() else {
                unknown += 1;
                sessions += 1;
                continue;
            };
            if reclaim_block_reason(&state, chrono::Utc::now(), Duration::ZERO, false).is_some() {
                continue;
            }
            if let Some(pid) = state.agent_pid {
                pids.insert(pid);
            } else {
                unknown += 1;
            }
            sessions += 1;
        }
        (pids.len() + unknown, sessions)
    }
}
