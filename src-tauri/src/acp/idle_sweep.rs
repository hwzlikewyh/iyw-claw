//! Periodic sweeper that disconnects ACP connections idle past a deadline.
//!
//! Connections accumulate when frontends close their window/tab without
//! triggering an explicit disconnect — common in web mode (browser tab
//! close has no server-side hook), and possible on desktop after panics.
//! The sweep prevents long-lived processes from leaking ACP child
//! processes, file handles, and memory.

use std::time::Duration;

use sea_orm::DatabaseConnection;

use crate::acp::manager::ConnectionManager;
use crate::commands::idle_agent_settings::{
    get_idle_agent_settings_core, DEFAULT_MAX_IDLE_CONNECTIONS,
};

/// Optional abandoned-connection timeout. It is disabled unless the operator
/// explicitly sets `IYW_CLAW_ACP_IDLE_TIMEOUT_SECS`; normal memory governance
/// and the user-selected idle count are handled separately. The sweep only
/// runs against recoverable `Connected` sessions with no protected work.
/// Agent events bump `last_activity_at`; frontend visibility and pending-input
/// keepalives renew separate short leases, so they protect a live surface
/// without corrupting LRU.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 1800;
/// Default prompt-stall recovery threshold (10 minutes without a single agent
/// event while `Prompting`). Recovery requests one safe cancellation and never
/// replays the prompt.
/// Override via `IYW_CLAW_ACP_PROMPT_STALL_TIMEOUT_SECS` (`0` disables).
pub const DEFAULT_PROMPT_STALL_TIMEOUT_SECS: u64 = 600;
/// Sweep cadence — runs once per minute. Each tick is a brief lock on the
/// connections map plus per-state `try_read`s, so a 1-minute interval is
/// trivially cheap relative to the wall-clock idle threshold.
pub const SWEEP_INTERVAL_SECS: u64 = 30;

/// Read the optional idle timeout from `IYW_CLAW_ACP_IDLE_TIMEOUT_SECS`.
/// Absence or `0` disables time-based reclaim; an unparseable explicit value
/// enables the 30-minute fallback.
pub fn idle_timeout_from_env() -> Option<Duration> {
    let raw = std::env::var("IYW_CLAW_ACP_IDLE_TIMEOUT_SECS").ok()?;
    let secs = raw.parse::<u64>().unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);
    (secs > 0).then_some(Duration::from_secs(secs))
}

/// Read the prompt-stall timeout from `IYW_CLAW_ACP_PROMPT_STALL_TIMEOUT_SECS`,
/// same semantics as [`idle_timeout_from_env`].
pub fn prompt_stall_timeout_from_env() -> Option<Duration> {
    duration_from_env(
        "IYW_CLAW_ACP_PROMPT_STALL_TIMEOUT_SECS",
        DEFAULT_PROMPT_STALL_TIMEOUT_SECS,
    )
}

fn duration_from_env(key: &str, default_secs: u64) -> Option<Duration> {
    let secs = match std::env::var(key) {
        Ok(raw) => raw.parse::<u64>().unwrap_or(default_secs),
        Err(_) => default_secs,
    };
    if secs == 0 {
        return None;
    }
    Some(Duration::from_secs(secs))
}

/// Read the idle-process fallback from `IYW_CLAW_ACP_MAX_IDLE_CONNECTIONS`.
/// `0` disables the count cap; absence defaults to four.
pub fn max_idle_connections_from_env() -> Option<usize> {
    let count = match std::env::var("IYW_CLAW_ACP_MAX_IDLE_CONNECTIONS") {
        Ok(raw) => raw.parse::<usize>().unwrap_or(DEFAULT_MAX_IDLE_CONNECTIONS),
        Err(_) => DEFAULT_MAX_IDLE_CONNECTIONS,
    };
    (count > 0).then_some(count)
}

/// Long-running task that calls `ConnectionManager::sweep_idle` on a
/// fixed interval. The caller spawns the returned future onto whichever
/// runtime they manage (`tokio::spawn` from inside an async context,
/// `tauri::async_runtime::spawn` from a Tauri `setup` callback that runs
/// outside the runtime).
///
/// Never exits on its own — the caller drops the spawned handle when
/// shutting down (process exit cleans up everything).
pub async fn idle_sweep_task(
    manager: ConnectionManager,
    db: DatabaseConnection,
    idle_timeout: Option<Duration>,
    stall_timeout: Option<Duration>,
    fallback_max_idle: Option<usize>,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // First `tick().await` returns immediately. Skip it so we don't
    // sweep at startup before any connections have a chance to settle.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let mut reclaimed = 0;
        if let Some(idle_timeout) = idle_timeout {
            let n = manager.sweep_idle(idle_timeout).await;
            reclaimed += n;
            if n > 0 {
                tracing::info!("[ACP] idle sweep disconnected {n} connection(s)");
            }
        }
        if reclaimed == 0 {
            let max_idle = match get_idle_agent_settings_core(&db).await {
                Ok(settings) => settings.max_idle_connections,
                Err(error) => {
                    tracing::warn!(error = %error, "[ACP] idle agent preference unavailable; using environment fallback");
                    fallback_max_idle
                }
            };
            let n = manager.sweep_excess_idle(max_idle).await;
            if n > 0 {
                tracing::info!(
                    max_idle = ?max_idle,
                    "[ACP] idle-capacity sweep disconnected {n} connection(s)"
                );
            }
        }
        if let Some(stall_timeout) = stall_timeout {
            let n = manager.recover_stalled_prompts(stall_timeout).await;
            if n > 0 {
                tracing::info!("[ACP] stall recovery requested for {n} silent prompt(s)");
            }
        }
    }
}
