use std::time::{Duration, Instant};

use super::ControlGate;
use crate::browser::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use crate::browser::user_control_lease::UserControlLease;

const USER_TAKEOVER_TIMEOUT: Duration = Duration::from_secs(3);

impl ControlGate {
    pub async fn reset_agent_access(&self, enabled: bool) {
        let mut inner = self.inner.lock().await;
        inner.agent_enabled = enabled;
        inner.epoch = inner.epoch.saturating_add(1);
        if let Some(active) = &inner.active_agent {
            active.cancellation.cancel();
        }
        inner.queue.clear();
        drop(inner);
        self.notify.notify_waiters();
    }

    pub async fn acquire_user(&self) -> Result<UserControlLease, BrowserError> {
        let (lease, active_agent) = {
            let mut inner = self.inner.lock().await;
            if inner.closed {
                return Err(tab_gone());
            }
            inner.epoch = inner.epoch.saturating_add(1);
            inner.active_user_operations = inner.active_user_operations.saturating_add(1);
            let active_agent = inner.active_agent.as_ref().map(|active| {
                active.cancellation.cancel();
                (active.operation_id.clone(), inner.epoch)
            });
            (UserControlLease::new(self.clone()), active_agent)
        };
        self.notify.notify_waiters();

        let Some((operation_id, epoch)) = active_agent else {
            return Ok(lease);
        };
        if let Err(error) = self.wait_for_agent_release(&operation_id, epoch).await {
            lease.finish().await;
            return Err(error);
        }
        Ok(lease)
    }

    async fn wait_for_agent_release(
        &self,
        operation_id: &str,
        epoch: u64,
    ) -> Result<(), BrowserError> {
        let deadline = Instant::now() + USER_TAKEOVER_TIMEOUT;
        loop {
            let notified = self.notify.notified();
            {
                let inner = self.inner.lock().await;
                if inner.closed {
                    return Err(tab_gone());
                }
                if inner.active_agent.is_none() {
                    return Ok(());
                }
            }
            let now = Instant::now();
            if now >= deadline
                || tokio::time::timeout(deadline - now, notified)
                    .await
                    .is_err()
            {
                return Err(takeover_timeout(operation_id, epoch));
            }
        }
    }
}

fn tab_gone() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserTabGone,
        "The browser tab is closed",
    )
}

fn takeover_timeout(operation_id: &str, epoch: u64) -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserControlChanged,
        "The Agent did not release browser control before user takeover timed out",
    )
    .with_context(BrowserErrorContext {
        operation_id: Some(operation_id.to_string()),
        control_epoch: Some(epoch),
        ..BrowserErrorContext::default()
    })
    .retryable(true)
}
