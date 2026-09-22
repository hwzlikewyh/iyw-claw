use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const TOOLS_READY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolReadinessError {
    Cancelled,
    TransportChanged,
    TimedOut,
}

impl ToolReadinessError {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "MCP startup cancelled",
            Self::TransportChanged => "MCP transport changed during startup",
            Self::TimedOut => "Agent did not complete MCP tools/list before the startup deadline",
        }
    }
}

#[derive(Default, Debug)]
struct ReadyState {
    generation: u64,
    delivered: bool,
}

#[derive(Clone, Default, Debug)]
pub(crate) struct ToolReadiness {
    state: Arc<Mutex<ReadyState>>,
    changed: Arc<Notify>,
}

impl ToolReadiness {
    pub(crate) fn generation(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .generation
    }

    pub(crate) fn reset(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.delivered = false;
        drop(state);
        self.changed.notify_waiters();
    }

    pub(super) fn delivered(&self, generation: u64) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.generation != generation {
            return;
        }
        state.delivered = true;
        drop(state);
        self.changed.notify_waiters();
    }

    pub(crate) async fn wait(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<(), ToolReadinessError> {
        self.wait_with_timeout(cancellation, TOOLS_READY_TIMEOUT)
            .await
    }

    async fn wait_with_timeout(
        &self,
        cancellation: &CancellationToken,
        timeout: Duration,
    ) -> Result<(), ToolReadinessError> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(ToolReadinessError::Cancelled),
            result = tokio::time::timeout(timeout, self.wait_delivered()) =>
                result.unwrap_or(Err(ToolReadinessError::TimedOut)),
        }
    }

    async fn wait_delivered(&self) -> Result<(), ToolReadinessError> {
        let generation = self.generation();
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                if state.generation != generation {
                    return Err(ToolReadinessError::TransportChanged);
                }
                if state.delivered {
                    return Ok(());
                }
            }
            changed.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolReadiness, ToolReadinessError};
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn timeout_is_distinguished_from_cancellation() {
        let readiness = ToolReadiness::default();
        let cancellation = CancellationToken::new();
        assert_eq!(
            readiness
                .wait_with_timeout(&cancellation, Duration::from_millis(1))
                .await,
            Err(ToolReadinessError::TimedOut)
        );

        cancellation.cancel();
        assert_eq!(
            readiness
                .wait_with_timeout(&cancellation, Duration::from_secs(1))
                .await,
            Err(ToolReadinessError::Cancelled)
        );
    }
}
