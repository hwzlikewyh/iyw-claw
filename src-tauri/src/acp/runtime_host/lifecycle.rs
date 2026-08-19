use std::ops::Deref;
use std::sync::{atomic::Ordering, Arc, Weak};
use std::time::Duration;

use super::{AgentRuntimeHost, HOST_IDLE_TIMEOUT, HOST_SHUTDOWN_TIMEOUT};
use crate::acp::runtime_host_registry::startup::HostDriverShutdownReport;

pub(crate) struct RuntimeHostReservation {
    host: Arc<AgentRuntimeHost>,
    armed: bool,
    shared: bool,
}

impl RuntimeHostReservation {
    pub(super) fn new(host: Arc<AgentRuntimeHost>) -> Self {
        Self {
            host,
            armed: true,
            shared: false,
        }
    }

    pub(super) fn new_shared(host: Arc<AgentRuntimeHost>) -> Self {
        Self {
            host,
            armed: true,
            shared: true,
        }
    }

    pub(super) fn clone_host(&self) -> Arc<AgentRuntimeHost> {
        Arc::clone(&self.host)
    }

    pub(super) fn mark_published(&mut self) {
        self.host.mark_published();
        self.shared = true;
    }

    pub(crate) fn register_route(
        &mut self,
        connection_id: String,
        session_id: Option<String>,
        route: super::RuntimeSessionRoute,
    ) -> Result<super::RuntimeHostRouteLease, crate::acp::error::AcpError> {
        let lease =
            self.host
                .register_reserved_route(connection_id, session_id, route, self.shared)?;
        self.armed = false;
        Ok(lease)
    }
}

impl Deref for RuntimeHostReservation {
    type Target = AgentRuntimeHost;

    fn deref(&self) -> &Self::Target {
        &self.host
    }
}

impl Drop for RuntimeHostReservation {
    fn drop(&mut self) {
        if self.armed {
            self.host.release_route_reservation(self.shared);
        }
    }
}

impl AgentRuntimeHost {
    pub(crate) fn release_route_reservation(self: &Arc<Self>, schedule_idle: bool) {
        let epoch = {
            let _guard = self
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self
                .reservations
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    count.checked_sub(1)
                })
                .is_err()
            {
                tracing::debug!(
                    agent = %self.key.agent_type,
                    fingerprint = self.key.fingerprint_prefix(),
                    "[ACP][host] route reservation already released"
                );
                return;
            }
            if self.active_route_count() != 0
                || self.reservations.load(Ordering::Acquire) != 0
                || !self.is_healthy()
            {
                return;
            }
            self.route_epoch.fetch_add(1, Ordering::AcqRel) + 1
        };
        if schedule_idle {
            self.schedule_idle_retirement(epoch);
        }
    }

    pub(super) fn route_released(host: Weak<Self>, schedule_idle: bool) {
        let Some(host) = host.upgrade() else {
            return;
        };
        let epoch = {
            let _guard = host
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let was_last_route = host
                .active_routes
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    count.checked_sub(1)
                })
                .is_ok_and(|previous| previous == 1);
            if !was_last_route
                || host.reservations.load(Ordering::Acquire) != 0
                || !host.is_healthy()
            {
                return;
            }
            host.route_epoch.fetch_add(1, Ordering::AcqRel) + 1
        };
        if schedule_idle {
            host.schedule_idle_retirement(epoch);
        }
    }

    fn schedule_idle_retirement(self: &Arc<Self>, epoch: u64) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.shutdown_if_idle_without_runtime(epoch);
            return;
        };
        let _guard = self
            .route_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.is_idle_epoch(epoch) {
            return;
        }
        let host = Arc::downgrade(self);
        let task = runtime.spawn(async move {
            tokio::time::sleep(HOST_IDLE_TIMEOUT).await;
            if let Some(host) = host.upgrade() {
                host.shutdown_if_idle(epoch).await;
            }
        });
        let mut current = self
            .idle_retirement
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(previous) = current.replace(task.abort_handle()) {
            previous.abort();
        }
    }

    fn shutdown_if_idle_without_runtime(&self, epoch: u64) {
        let _guard = self
            .route_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.is_idle_epoch(epoch) {
            self.healthy.store(false, Ordering::Release);
            self.shutdown.cancel();
        }
    }

    fn is_idle_epoch(&self, epoch: u64) -> bool {
        self.active_route_count() == 0
            && self.reservations.load(Ordering::Acquire) == 0
            && self.route_epoch.load(Ordering::Acquire) == epoch
            && self.is_healthy()
    }

    async fn shutdown_if_idle(&self, epoch: u64) {
        {
            let _guard = self
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !self.is_idle_epoch(epoch) {
                return;
            }
            self.healthy.store(false, Ordering::Release);
            self.shutdown.cancel();
        }
        tracing::info!(
            agent = %self.key.agent_type,
            fingerprint = self.key.fingerprint_prefix(),
            pid = self.pid().unwrap_or_default(),
            idle_ms = HOST_IDLE_TIMEOUT.as_millis(),
            "[ACP][host] idle runtime Host retired"
        );
        let _ = self.finish_shutdown().await;
    }

    pub(crate) fn log_acquired(&self, outcome: &str, elapsed: Duration) {
        tracing::info!(
            agent = %self.key.agent_type,
            fingerprint = self.key.fingerprint_prefix(),
            outcome,
            wait_ms = elapsed.as_millis(),
            age_ms = self.created_at.elapsed().as_millis(),
            pid = self.pid().unwrap_or_default(),
            "[ACP][host] runtime Host acquired"
        );
    }

    pub(crate) async fn shutdown(&self) -> HostDriverShutdownReport {
        {
            let _guard = self
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.cancel_idle_retirement();
            self.healthy.store(false, Ordering::Release);
            self.shutdown.cancel();
        }
        self.finish_shutdown().await
    }

    async fn finish_shutdown(&self) -> HostDriverShutdownReport {
        let report = self.driver.shutdown_and_reap(HOST_SHUTDOWN_TIMEOUT).await;
        if report.timed_out {
            tracing::warn!(
                agent = %self.key.agent_type,
                pid = self.pid().unwrap_or_default(),
                "[ACP][host] runtime Host shutdown timed out; driver aborted"
            );
        }
        if !report.clean {
            tracing::error!(
                agent = %self.key.agent_type,
                pid = self.pid().unwrap_or_default(),
                reaped = report.reaped,
                "[ACP][host] runtime Host driver stopped uncleanly"
            );
        }
        report
    }
}
