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

/// Default idle threshold (3 minutes). Override at startup via
/// `IYW_CLAW_ACP_IDLE_TIMEOUT_SECS`. The sweep only runs against
/// connections in `Connected` state with no `pending_permission`, and
/// `last_activity_at` is bumped on every emit and on every frontend
/// keepalive touch (~30s cadence for open tabs), so an actively-used
/// or visible connection never qualifies.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 180;
/// Default prompt-stall threshold (10 minutes without a single agent event
/// while `Prompting`). Long enough for slow model turns and silent tool runs;
/// short enough that a hung upstream doesn't spin "generating" for hours.
/// Override via `IYW_CLAW_ACP_PROMPT_STALL_TIMEOUT_SECS` (`0` disables).
pub const DEFAULT_PROMPT_STALL_TIMEOUT_SECS: u64 = 600;
/// Default cap on idle resident agent processes (finished conversations).
/// Prompting sessions never count. Override via
/// `IYW_CLAW_ACP_MAX_IDLE_CONNECTIONS` (`0` disables the cap).
pub const DEFAULT_MAX_IDLE_CONNECTIONS: usize = 3;
/// Sweep cadence — runs once per minute. Each tick is a brief lock on the
/// connections map plus per-state `try_read`s, so a 1-minute interval is
/// trivially cheap relative to the wall-clock idle threshold.
pub const SWEEP_INTERVAL_SECS: u64 = 60;

/// Read the idle timeout from `IYW_CLAW_ACP_IDLE_TIMEOUT_SECS`, falling back
/// to `DEFAULT_IDLE_TIMEOUT_SECS`. A `0` value disables the sweep
/// (returns `None`); any unparseable value is treated as "use default".
pub fn idle_timeout_from_env() -> Option<Duration> {
    duration_from_env("IYW_CLAW_ACP_IDLE_TIMEOUT_SECS", DEFAULT_IDLE_TIMEOUT_SECS)
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

/// Read the idle-process cap from `IYW_CLAW_ACP_MAX_IDLE_CONNECTIONS`;
/// `0` disables capping, unparseable values fall back to the default.
pub fn max_idle_connections_from_env() -> Option<usize> {
    let count = match std::env::var("IYW_CLAW_ACP_MAX_IDLE_CONNECTIONS") {
        Ok(raw) => raw
            .parse::<usize>()
            .unwrap_or(DEFAULT_MAX_IDLE_CONNECTIONS),
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
    max_idle: Option<usize>,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // First `tick().await` returns immediately. Skip it so we don't
    // sweep at startup before any connections have a chance to settle.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if let Some(idle_timeout) = idle_timeout {
            let n = manager.sweep_idle(idle_timeout).await;
            if n > 0 {
                tracing::info!("[ACP] idle sweep disconnected {n} connection(s)");
            }
        }
        if let Some(max_idle) = max_idle {
            let n = manager.sweep_excess_idle(max_idle).await;
            if n > 0 {
                tracing::info!(
                    "[ACP] idle-capacity sweep disconnected {n} connection(s) (cap {max_idle})"
                );
            }
        }
        if let Some(stall_timeout) = stall_timeout {
            let n = manager.sweep_stalled_prompts(&db, stall_timeout).await;
            if n > 0 {
                tracing::info!("[ACP] stall sweep cancelled {n} hung prompt(s)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Single test sequences all env-var assertions to avoid the
    /// notorious parallel-test race on shared environment state. Cargo
    /// runs tests in parallel by default; setting `IYW_CLAW_ACP_IDLE_TIMEOUT_SECS`
    /// in concurrent tests would interleave with each other.
    #[test]
    fn idle_timeout_env_parsing() {
        // Disabled when zero.
        std::env::set_var("IYW_CLAW_ACP_IDLE_TIMEOUT_SECS", "0");
        assert!(idle_timeout_from_env().is_none());

        // Falls back to default when unparseable.
        std::env::set_var("IYW_CLAW_ACP_IDLE_TIMEOUT_SECS", "not-a-number");
        assert_eq!(
            idle_timeout_from_env().unwrap().as_secs(),
            DEFAULT_IDLE_TIMEOUT_SECS
        );

        // Uses provided value when it parses.
        std::env::set_var("IYW_CLAW_ACP_IDLE_TIMEOUT_SECS", "120");
        assert_eq!(idle_timeout_from_env().unwrap().as_secs(), 120);

        // Falls back to default when unset.
        std::env::remove_var("IYW_CLAW_ACP_IDLE_TIMEOUT_SECS");
        assert_eq!(
            idle_timeout_from_env().unwrap().as_secs(),
            DEFAULT_IDLE_TIMEOUT_SECS
        );
    }
}
