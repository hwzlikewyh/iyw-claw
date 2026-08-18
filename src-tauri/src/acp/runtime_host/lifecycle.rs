use std::sync::{atomic::Ordering, Arc, Weak};
use std::time::Duration;

use super::{AgentRuntimeHost, HOST_IDLE_TIMEOUT, HOST_SHUTDOWN_TIMEOUT};

impl AgentRuntimeHost {
    pub(crate) fn release_route_reservation(self: &Arc<Self>) {
        let epoch = {
            let _guard = self
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.reservations.fetch_sub(1, Ordering::AcqRel) == 0 {
                tracing::debug!(
                    agent = %self.key.agent_type,
                    fingerprint = self.key.fingerprint_prefix(),
                    "[ACP][host] route reservation already released"
                );
                return;
            }
            if self.active_route_count() != 0 || self.reservations.load(Ordering::Acquire) != 0 {
                return;
            }
            self.route_epoch.fetch_add(1, Ordering::AcqRel) + 1
        };
        self.schedule_idle_retirement(epoch);
    }

    pub(super) fn route_released(host: Weak<Self>) {
        let Some(host) = host.upgrade() else {
            return;
        };
        let epoch = {
            let _guard = host
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if host.active_routes.fetch_sub(1, Ordering::AcqRel) != 1
                || host.reservations.load(Ordering::Acquire) != 0
            {
                return;
            }
            host.route_epoch.fetch_add(1, Ordering::AcqRel) + 1
        };
        host.schedule_idle_retirement(epoch);
    }

    fn schedule_idle_retirement(self: &Arc<Self>, epoch: u64) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.healthy.store(false, Ordering::Release);
            self.shutdown.cancel();
            return;
        };
        let host = Arc::clone(self);
        runtime.spawn(async move {
            tokio::time::sleep(HOST_IDLE_TIMEOUT).await;
            host.shutdown_if_idle(epoch).await;
        });
    }

    async fn shutdown_if_idle(&self, epoch: u64) {
        {
            let _guard = self
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.active_route_count() != 0
                || self.reservations.load(Ordering::Acquire) != 0
                || self.route_epoch.load(Ordering::Acquire) != epoch
                || !self.is_healthy()
            {
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
        self.finish_shutdown().await;
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

    pub(crate) async fn shutdown(&self) {
        {
            let _guard = self
                .route_guard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.healthy.store(false, Ordering::Release);
            self.shutdown.cancel();
        }
        self.finish_shutdown().await;
    }

    async fn finish_shutdown(&self) {
        let Some(mut driver) = self.driver.lock().await.take() else {
            return;
        };
        tokio::select! {
            _ = &mut driver => {}
            _ = tokio::time::sleep(HOST_SHUTDOWN_TIMEOUT) => {
                driver.abort();
                tracing::warn!(
                    agent = %self.key.agent_type,
                    pid = self.pid().unwrap_or_default(),
                    "[ACP][host] runtime Host shutdown timed out; driver aborted"
                );
            }
        }
    }
}
