use crate::acp::manager::ConnectionManager;
use crate::acp::resource_governor::{MemoryPressure, ResourceSnapshot, SystemMemorySnapshot};
use tokio_util::sync::CancellationToken;

use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::runtime::BrowserRuntime;

pub(super) struct BrowserResourceGovernor {
    connections: ConnectionManager,
}

impl std::fmt::Debug for BrowserResourceGovernor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserResourceGovernor")
            .finish_non_exhaustive()
    }
}

impl BrowserResourceGovernor {
    pub(super) fn new(connections: ConnectionManager) -> Self {
        Self { connections }
    }

    pub(super) async fn guard_runtime_start(
        &self,
        runtime: &BrowserRuntime,
        cancellation: &CancellationToken,
    ) -> Result<(), BrowserError> {
        ensure_not_cancelled(cancellation)?;
        let stale_processes = runtime.reclaim_stale_profile().await?;
        ensure_not_cancelled(cancellation)?;
        let before = ResourceSnapshot::capture().memory;
        let reclaimed_agents = if under_pressure(before.pressure) {
            self.connections.sweep_excess_idle(Some(0)).await
        } else {
            0
        };
        let after = if under_pressure(before.pressure) {
            ResourceSnapshot::capture().memory
        } else {
            before
        };
        ensure_not_cancelled(cancellation)?;
        log_runtime_gate(before, after, reclaimed_agents, stale_processes);
        if under_pressure(after.pressure) {
            return Err(insufficient_memory(after, "runtime_start"));
        }
        Ok(())
    }

    pub(super) fn guard_new_tab(&self) -> Result<(), BrowserError> {
        let memory = ResourceSnapshot::capture().memory;
        if memory.pressure == MemoryPressure::Emergency {
            tracing::warn!(
                target: "iyw_claw_browser",
                pressure = memory.pressure.as_str(),
                available_bytes = memory.available_bytes,
                total_bytes = memory.total_bytes,
                "browser tab creation blocked by memory pressure"
            );
            return Err(insufficient_memory(memory, "new_tab"));
        }
        Ok(())
    }
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), BrowserError> {
    if cancellation.is_cancelled() {
        return Err(BrowserError::shutting_down());
    }
    Ok(())
}

fn under_pressure(pressure: MemoryPressure) -> bool {
    matches!(
        pressure,
        MemoryPressure::Shrinking | MemoryPressure::Emergency
    )
}

fn insufficient_memory(memory: SystemMemorySnapshot, operation: &str) -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInsufficientMemory,
        "Available system memory is too low to open the built-in browser",
    )
    .retryable(true)
    .with_context(BrowserErrorContext {
        memory_operation: Some(operation.to_string()),
        memory_pressure: Some(memory.pressure.as_str().to_string()),
        available_memory_bytes: Some(memory.available_bytes),
        total_memory_bytes: Some(memory.total_bytes),
        shrinking_reserve_bytes: Some(memory.shrinking_reserve_bytes),
        emergency_reserve_bytes: Some(memory.emergency_reserve_bytes),
        ..BrowserErrorContext::default()
    })
}

fn log_runtime_gate(
    before: SystemMemorySnapshot,
    after: SystemMemorySnapshot,
    reclaimed_agents: usize,
    stale_processes: usize,
) {
    tracing::info!(
        target: "iyw_claw_browser",
        pressure_before = before.pressure.as_str(),
        pressure_after = after.pressure.as_str(),
        available_before_bytes = before.available_bytes,
        available_after_bytes = after.available_bytes,
        total_bytes = after.total_bytes,
        reclaimed_agents,
        stale_processes,
        allowed = !under_pressure(after.pressure),
        "browser runtime memory gate completed"
    );
}
