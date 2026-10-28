use std::collections::HashMap;

use super::ConnectionManager;
use crate::acp::resource_governor::{self, MemoryPressure, ResourceSnapshot, SystemMemorySnapshot};

const MAX_EMERGENCY_RECLAIMS_PER_TICK: usize = 4;

struct IdleCandidate {
    connection_id: String,
    last_activity_at: chrono::DateTime<chrono::Utc>,
    launcher_pid: Option<u32>,
    private_bytes: Option<u64>,
}

impl ConnectionManager {
    /// 每轮只观察一次内存压力；紧急回收逐个执行，并重新采样可用内存。
    pub async fn sweep_excess_idle(&self, max_keep: Option<usize>) -> usize {
        let Some(mut memory) = self.governed_memory_snapshot().await else {
            return 0;
        };
        self.retire_unused_prewarm(memory.pressure).await;
        let max_keep = self.remaining_idle_connection_budget(max_keep).await;
        let emergency_batch = memory.pressure == MemoryPressure::Emergency;
        let pressure = memory.pressure;
        let mut disconnected = 0;
        loop {
            let reclaimed = self.sweep_excess_idle_once(max_keep, memory).await;
            disconnected += reclaimed;
            if reclaimed == 0 || !emergency_batch || disconnected >= MAX_EMERGENCY_RECLAIMS_PER_TICK
            {
                break;
            }
            let Some(current) = sample_memory().await else {
                break;
            };
            if current.pressure != MemoryPressure::Emergency {
                break;
            }
            memory = SystemMemorySnapshot {
                pressure,
                ..current
            };
            tracing::info!(
                disconnected,
                available_bytes = memory.available_bytes,
                "[ACP] emergency memory reclaim pass continuing"
            );
        }
        disconnected
    }

    async fn governed_memory_snapshot(&self) -> Option<SystemMemorySnapshot> {
        let mut memory = sample_memory().await?;
        let mut tracker = self.memory_pressure_tracker.lock().await;
        let previous = tracker.current();
        memory.pressure = tracker.observe(memory.total_bytes, memory.available_bytes);
        if memory.pressure != previous && memory.pressure != MemoryPressure::Unknown {
            tracing::info!(
                previous = previous.as_str(),
                current = memory.pressure.as_str(),
                available_bytes = memory.available_bytes,
                total_bytes = memory.total_bytes,
                "[ACP] memory pressure state changed"
            );
        }
        Some(memory)
    }

    async fn retire_unused_prewarm(&self, pressure: MemoryPressure) {
        if !under_pressure(pressure) {
            return;
        }
        // 用户可能正在接管预热实例；启动准入繁忙时不排队、不打断启动。
        let Ok(_admission) = self.speculative_runtime_gate.try_lock() else {
            return;
        };
        self.runtime_hosts.retire_unused_excess(0).await;
    }

    async fn sweep_excess_idle_once(
        &self,
        max_keep: Option<usize>,
        memory: SystemMemorySnapshot,
    ) -> usize {
        let keep_limit = match (
            max_keep,
            resource_governor::idle_keep_limit(memory.pressure),
        ) {
            (Some(user), Some(pressure)) => Some(user.min(pressure)),
            (None, pressure) => pressure,
            (user, None) => user,
        };
        if keep_limit.is_none() && !under_pressure(memory.pressure) {
            return 0;
        }
        let allow_recent =
            resource_governor::is_hard_emergency(memory.total_bytes, memory.available_bytes);
        let candidates = self.idle_reclaim_candidates(allow_recent).await;
        let Some(id) = select_idle_candidate(candidates, keep_limit, memory).await else {
            return 0;
        };
        tracing::info!(
            connection_id = id,
            pressure = memory.pressure.as_str(),
            available_bytes = memory.available_bytes,
            total_bytes = memory.total_bytes,
            shrinking_reserve_bytes = memory.shrinking_reserve_bytes,
            emergency_reserve_bytes = memory.emergency_reserve_bytes,
            keep_limit = ?keep_limit,
            private_budget = resource_governor::idle_private_budget(memory.total_bytes),
            "[ACP] resource governor disconnecting idle connection"
        );
        // 采样期间可能开始新回合或获得可见租约，实际断开前仍按原入口复核。
        usize::from(
            self.disconnect_if_reclaimable(
                &id,
                resource_governor::completion_grace(),
                allow_recent,
                "memory_governor",
            )
            .await
            .unwrap_or(false),
        )
    }

    async fn idle_reclaim_candidates(&self, allow_recent: bool) -> Vec<IdleCandidate> {
        let states: Vec<_> = self
            .connections
            .lock()
            .await
            .iter()
            .map(|(id, connection)| (id.clone(), connection.state.clone()))
            .collect();
        let now = chrono::Utc::now();
        states
            .into_iter()
            .filter_map(|(connection_id, state)| {
                let state = state.try_read().ok()?;
                if resource_governor::reclaim_block_reason(
                    &state,
                    now,
                    resource_governor::completion_grace(),
                    allow_recent,
                )
                .is_some()
                {
                    return None;
                }
                Some(IdleCandidate {
                    connection_id,
                    last_activity_at: state.last_activity_at,
                    launcher_pid: state.agent_pid,
                    private_bytes: None,
                })
            })
            .collect()
    }
}

fn under_pressure(pressure: MemoryPressure) -> bool {
    matches!(
        pressure,
        MemoryPressure::Shrinking | MemoryPressure::Emergency
    )
}

async fn sample_memory() -> Option<SystemMemorySnapshot> {
    match tokio::task::spawn_blocking(resource_governor::current_memory_snapshot).await {
        Ok(memory) => Some(memory),
        Err(error) => {
            tracing::warn!(%error, "[ACP] memory sampling failed; skipping idle reclaim");
            None
        }
    }
}

async fn select_idle_candidate(
    mut candidates: Vec<IdleCandidate>,
    keep_limit: Option<usize>,
    memory: SystemMemorySnapshot,
) -> Option<String> {
    let count_exceeded = keep_limit.is_some_and(|limit| candidates.len() > limit);
    if candidates.is_empty() || (!count_exceeded && !under_pressure(memory.pressure)) {
        return None;
    }
    measure_idle_memory(&mut candidates).await;
    let private_total = candidates
        .iter()
        .filter_map(|item| item.private_bytes)
        .reduce(u64::saturating_add);
    let budget_exceeded = under_pressure(memory.pressure)
        && private_total.is_some_and(|total| {
            total > resource_governor::idle_private_budget(memory.total_bytes)
        });
    if !count_exceeded && !budget_exceeded {
        return None;
    }
    candidates.sort_by(|left, right| {
        left.last_activity_at
            .cmp(&right.last_activity_at)
            .then_with(|| {
                resource_governor::compare_reclaim_memory(left.private_bytes, right.private_bytes)
            })
    });
    candidates.into_iter().next().map(|item| item.connection_id)
}

async fn measure_idle_memory(candidates: &mut [IdleCandidate]) {
    let pids: Vec<_> = candidates
        .iter()
        .filter_map(|item| item.launcher_pid)
        .collect();
    // 进程内星河会话没有独立 PID；没有可测量对象时不扫描系统进程。
    if pids.is_empty() || !cfg!(target_os = "windows") {
        return;
    }
    let measurements = tokio::task::spawn_blocking(move || {
        let resources = ResourceSnapshot::capture();
        let mut measurements = HashMap::new();
        for pid in pids {
            measurements
                .entry(pid)
                .or_insert_with(|| resources.connection_memory(Some(pid)).private_bytes);
        }
        measurements
    })
    .await;
    match measurements {
        Ok(measurements) => {
            for item in candidates {
                item.private_bytes = item
                    .launcher_pid
                    .and_then(|pid| measurements.get(&pid).copied().flatten());
            }
        }
        Err(error) => {
            tracing::warn!(%error, "[ACP] idle process sampling failed; applying count limit only")
        }
    }
}
