use tokio_util::sync::CancellationToken;

use super::control::ControlGate;
use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};

pub struct AgentControlLease {
    gate: ControlGate,
    operation_id: String,
    epoch: u64,
    cancellation: CancellationToken,
    completed: bool,
}

impl AgentControlLease {
    pub(super) fn new(
        gate: ControlGate,
        operation_id: String,
        epoch: u64,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            gate,
            operation_id,
            epoch,
            cancellation,
            completed: false,
        }
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn cancellation_error(&self) -> BrowserError {
        BrowserError::new(
            BrowserErrorCode::BrowserControlChanged,
            "Browser control changed while the Agent operation was running",
        )
        .with_context(BrowserErrorContext {
            operation_id: Some(self.operation_id.clone()),
            control_epoch: Some(self.epoch),
            ..BrowserErrorContext::default()
        })
    }

    pub async fn finish(mut self) {
        self.completed = true;
        self.gate.complete_agent(&self.operation_id).await;
    }
}

impl Drop for AgentControlLease {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let gate = self.gate.clone();
        let operation_id = self.operation_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { gate.complete_agent(&operation_id).await });
        }
    }
}
