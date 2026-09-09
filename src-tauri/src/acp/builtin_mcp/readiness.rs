use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const TOOLS_READY_TIMEOUT: Duration = Duration::from_secs(20);

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

    pub(crate) async fn wait(&self, cancellation: &CancellationToken) -> Result<(), &'static str> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err("MCP startup cancelled"),
            result = tokio::time::timeout(TOOLS_READY_TIMEOUT, self.wait_delivered()) => {
                result.unwrap_or(Err("Agent did not complete MCP tools/list before the startup deadline"))
            }
        }
    }

    async fn wait_delivered(&self) -> Result<(), &'static str> {
        let generation = self.generation();
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                if state.generation != generation {
                    return Err("MCP transport changed during startup");
                }
                if state.delivered {
                    return Ok(());
                }
            }
            changed.await;
        }
    }
}
