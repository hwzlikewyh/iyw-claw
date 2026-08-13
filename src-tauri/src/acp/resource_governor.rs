//! Resource policy for idle ACP connections.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Duration;

use sysinfo::{Pid, System};

use crate::acp::session_state::SessionState;
use crate::acp::types::ConnectionStatus;
use crate::models::agent::AgentType;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const MIN_AGENT_BUDGET: u64 = 512 * MIB;
const MAX_AGENT_BUDGET: u64 = 1536 * MIB;
const AGENT_BUDGET_PERCENT: u64 = 5;
const SHRINKING_TARGET_PERCENT: u64 = 35;
const SHRINKING_TARGET_BYTES: u64 = 8 * GIB;
const SHRINKING_MAX_PERCENT: u64 = 50;
const SHRINKING_MAX_BYTES: u64 = 12 * GIB;
const EMERGENCY_TARGET_PERCENT: u64 = 25;
const EMERGENCY_TARGET_BYTES: u64 = 5 * GIB;
const EMERGENCY_MAX_BYTES: u64 = 8 * GIB;
const DEFAULT_IDLE_KEEP: usize = 2;
const SHRINK_IDLE_KEEP: usize = 1;
const COMPLETION_GRACE: i64 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    Comfortable,
    Shrinking,
    Emergency,
    Unknown,
}

impl MemoryPressure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Shrinking => "shrinking",
            Self::Emergency => "emergency",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SystemMemorySnapshot {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub pressure: MemoryPressure,
    pub shrinking_reserve_bytes: u64,
    pub emergency_reserve_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryReserveThresholds {
    pub shrinking_bytes: u64,
    pub emergency_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ConnectionMemorySnapshot {
    pub launcher_pid: Option<u32>,
    pub private_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct RuntimeSessionSnapshot {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub agent_type: AgentType,
    pub status: ConnectionStatus,
    pub launcher_pid: Option<u32>,
    pub last_activity_at: chrono::DateTime<chrono::Utc>,
    pub recoverable: bool,
    pub protection_reason: Option<&'static str>,
}

pub struct ResourceSnapshot {
    system: System,
    pub memory: SystemMemorySnapshot,
}

impl ResourceSnapshot {
    pub fn capture() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let total = system.total_memory();
        let available = system.available_memory();
        Self {
            system,
            memory: system_memory_snapshot(total, available),
        }
    }

    pub fn connection_memory(&self, state: &SessionState) -> ConnectionMemorySnapshot {
        let Some(pid) = state.agent_pid else {
            return ConnectionMemorySnapshot {
                launcher_pid: None,
                private_bytes: None,
            };
        };
        let descendants = descendant_pids(&self.system, Pid::from_u32(pid));
        let private_bytes = descendants
            .iter()
            .map(|pid| private_memory_bytes(pid.as_u32()))
            .collect::<Option<Vec<_>>>()
            .map(|values| values.iter().sum());
        ConnectionMemorySnapshot {
            launcher_pid: Some(pid),
            private_bytes,
        }
    }
}

pub fn memory_reserve_thresholds(total: u64) -> MemoryReserveThresholds {
    if total == 0 {
        return MemoryReserveThresholds {
            shrinking_bytes: 0,
            emergency_bytes: 0,
        };
    }
    let shrinking_bytes = (total.saturating_mul(SHRINKING_TARGET_PERCENT) / 100)
        .max(SHRINKING_TARGET_BYTES)
        .min(total.saturating_mul(SHRINKING_MAX_PERCENT) / 100)
        .min(SHRINKING_MAX_BYTES);
    let emergency_bytes = (total.saturating_mul(EMERGENCY_TARGET_PERCENT) / 100)
        .max(EMERGENCY_TARGET_BYTES)
        .min(total.saturating_mul(3) / 8)
        .min(EMERGENCY_MAX_BYTES);
    MemoryReserveThresholds {
        shrinking_bytes,
        emergency_bytes,
    }
}

pub fn system_memory_snapshot(total: u64, available: u64) -> SystemMemorySnapshot {
    let thresholds = memory_reserve_thresholds(total);
    SystemMemorySnapshot {
        total_bytes: total,
        available_bytes: available,
        pressure: classify_pressure(total, available),
        shrinking_reserve_bytes: thresholds.shrinking_bytes,
        emergency_reserve_bytes: thresholds.emergency_bytes,
    }
}

pub fn classify_pressure(total: u64, available: u64) -> MemoryPressure {
    if total == 0 {
        return MemoryPressure::Unknown;
    }
    let thresholds = memory_reserve_thresholds(total);
    if available < thresholds.emergency_bytes {
        return MemoryPressure::Emergency;
    }
    if available < thresholds.shrinking_bytes {
        return MemoryPressure::Shrinking;
    }
    MemoryPressure::Comfortable
}

pub fn idle_keep_limit(pressure: MemoryPressure) -> Option<usize> {
    match pressure {
        MemoryPressure::Comfortable => Some(DEFAULT_IDLE_KEEP),
        MemoryPressure::Shrinking => Some(SHRINK_IDLE_KEEP),
        MemoryPressure::Emergency => Some(0),
        MemoryPressure::Unknown => None,
    }
}

pub fn idle_private_budget(total_bytes: u64) -> u64 {
    (total_bytes.saturating_mul(AGENT_BUDGET_PERCENT) / 100)
        .clamp(MIN_AGENT_BUDGET, MAX_AGENT_BUDGET)
}

pub fn completion_grace() -> Duration {
    Duration::from_secs(COMPLETION_GRACE as u64)
}

pub fn compare_reclaim_memory(left: Option<u64>, right: Option<u64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => right.cmp(&left),
        _ => std::cmp::Ordering::Equal,
    }
}

fn active_work_block_reason(
    state: &SessionState,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<&'static str> {
    if state.turn_in_flight || state.turn_completion_pending {
        return Some("turn_in_progress");
    }
    if state.pending_permission.is_some() {
        return Some("permission_pending");
    }
    if state.pending_question.is_some() {
        return Some("question_pending");
    }
    if state.pending_channel_confirmation.is_some() {
        return Some("confirmation_pending");
    }
    if state.native_background_turn.is_some() {
        return Some("background_turn_active");
    }
    if !state.active_tool_calls.is_empty() {
        return Some("tool_call_active");
    }
    if !state.active_delegations.is_empty() {
        return Some("delegation_active");
    }
    if state.has_active_background_work(now) {
        return Some("background_task_active");
    }
    (state.active_terminal_count.load(Ordering::Acquire) > 0).then_some("terminal_active")
}

fn interaction_block_reason(
    state: &SessionState,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<&'static str> {
    if state.agent_inputs.iter().any(|item| !item.is_terminal()) {
        return Some("agent_input_pending");
    }
    if state
        .pending_input_lease_until
        .is_some_and(|expires_at| expires_at > now)
    {
        return Some("client_input_pending");
    }
    state
        .visible_lease_until
        .is_some_and(|expires_at| expires_at > now)
        .then_some("conversation_visible")
}

pub fn reclaim_block_reason(
    state: &SessionState,
    now: chrono::DateTime<chrono::Utc>,
    grace: Duration,
    allow_recent_activity: bool,
) -> Option<&'static str> {
    if state.status != ConnectionStatus::Connected {
        return Some("connection_not_idle");
    }
    if state.external_id.is_none() {
        return Some("session_not_linked");
    }
    if !state.recoverable_session {
        return Some("session_not_recoverable");
    }
    if state.recovery_failed {
        return Some("session_recovery_failed");
    }
    if let Some(reason) =
        active_work_block_reason(state, now).or_else(|| interaction_block_reason(state, now))
    {
        return Some(reason);
    }
    if allow_recent_activity {
        return None;
    }
    let grace = chrono::Duration::from_std(grace).ok()?;
    let last_used = state.last_activity_at.max(state.last_agent_event_at);
    (now.signed_duration_since(last_used) < grace).then_some("recently_active")
}

fn descendant_pids(system: &System, root: Pid) -> HashSet<Pid> {
    let mut pids = HashSet::from([root]);
    let mut changed = true;
    while changed {
        changed = false;
        for (pid, process) in system.processes() {
            if pids.contains(pid) {
                continue;
            }
            if process
                .parent()
                .is_some_and(|parent| pids.contains(&parent))
            {
                changed |= pids.insert(*pid);
            }
        }
    }
    pids
}

#[cfg(target_os = "windows")]
fn private_memory_bytes(pid: u32) -> Option<u64> {
    crate::commands::performance::performance_windows::private_commit_bytes(pid)
}

#[cfg(not(target_os = "windows"))]
fn private_memory_bytes(_pid: u32) -> Option<u64> {
    None
}
