use std::time::Duration;

use tokio::time::Instant;

use super::tasks::wait_for_task;
use super::BuiltinMcpService;
use crate::acp::builtin_mcp::lease::LeaseShutdownReport;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

impl BuiltinMcpService {
    pub async fn shutdown(&self) -> bool {
        self.quiesce();
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        let prelude = self.begin_cleanup(deadline).await;
        self.shutdown.cancel();
        let (service_tasks_reaped, mut report) = tokio::join!(
            self.reap_service_tasks(deadline),
            self.finish_cleanup(deadline),
        );
        if let (Some(report), Some((revoked, tasks_reaped))) = (&mut report, prelude) {
            report.merge_prelude(revoked, tasks_reaped);
        }
        log_shutdown(report.as_ref(), service_tasks_reaped)
    }

    async fn begin_cleanup(&self, deadline: Instant) -> Option<(usize, bool)> {
        match tokio::time::timeout_at(deadline, self.client.begin_revoke_all()).await {
            Ok(prelude) => Some(prelude),
            Err(_) => {
                tracing::error!(
                    target: "builtin_mcp",
                    timeout_ms = SHUTDOWN_TIMEOUT.as_millis(),
                    "HTTP MCP shutdown prelude timed out"
                );
                None
            }
        }
    }

    async fn finish_cleanup(&self, deadline: Instant) -> Option<LeaseShutdownReport> {
        match tokio::time::timeout_at(deadline, self.client.revoke_all()).await {
            Ok(report) => Some(report),
            Err(_) => {
                tracing::error!(
                    target: "builtin_mcp",
                    timeout_ms = SHUTDOWN_TIMEOUT.as_millis(),
                    "HTTP MCP durable cleanup timed out during shutdown"
                );
                None
            }
        }
    }

    async fn reap_service_tasks(&self, deadline: Instant) -> bool {
        // Keep handles registered until each wait completes. If an outer timeout
        // cancels this future, the next shutdown pass can still reap them.
        let mut joins = self.joins.lock().await;
        let mut complete = true;
        while let Some(index) = joins.len().checked_sub(1) {
            complete &= wait_for_task(&mut joins[index], deadline).await;
            joins.swap_remove(index);
        }
        complete
    }
}

fn log_shutdown(report: Option<&LeaseShutdownReport>, service_tasks_reaped: bool) -> bool {
    let cleanup_completed =
        report.is_some_and(LeaseShutdownReport::is_complete) && service_tasks_reaped;
    tracing::info!(
        target: "builtin_mcp",
        revoked_sessions = report.map_or(0, |report| report.revoked_sessions),
        authorities_closed = report.is_some_and(|report| report.authorities_closed),
        runtimes_empty = report.is_some_and(|report| report.runtimes_empty),
        bindings_empty = report.is_some_and(|report| report.bindings_empty),
        receipts_empty = report.is_some_and(|report| report.receipts_empty),
        pending_cleanup_empty = report.is_some_and(|report| report.pending_cleanup_empty),
        protocol_sessions_empty = report.is_some_and(|report| report.protocol_sessions_empty),
        parent_tasks_reaped = report.is_some_and(|report| report.parent_tasks_reaped),
        cleanup_tasks_reaped = report.is_some_and(|report| report.cleanup_tasks_reaped),
        service_tasks_reaped,
        cleanup_completed,
        "process HTTP MCP service stopped"
    );
    cleanup_completed
}
