use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::control_lease::AgentControlLease;
use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;

#[derive(Clone, Copy)]
pub(super) struct AgentToolContext<'a> {
    pub cancellation: &'a CancellationToken,
}

pub(super) struct AgentOperationCancellation {
    token: CancellationToken,
    bridge: JoinHandle<()>,
}

impl AgentOperationCancellation {
    pub fn new(request: &CancellationToken, control: CancellationToken) -> Self {
        let token = request.child_token();
        let bridged = token.clone();
        let bridge = tokio::spawn(async move {
            control.cancelled().await;
            bridged.cancel();
        });
        Self { token, bridge }
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for AgentOperationCancellation {
    fn drop(&mut self) {
        self.bridge.abort();
    }
}

impl BrowserSessionManager {
    pub(super) async fn acquire_agent_lease(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
    ) -> Result<AgentControlLease, BrowserError> {
        tokio::select! {
            _ = context.cancellation.cancelled() => Err(cancelled_error()),
            result = self.acquire_agent_control(tab_id) => result,
        }
    }
}

pub(super) fn ensure_request_active(context: AgentToolContext<'_>) -> Result<(), BrowserError> {
    (!context.cancellation.is_cancelled())
        .then_some(())
        .ok_or_else(cancelled_error)
}

pub(super) fn cancelled_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserCancelled,
        "The browser operation was cancelled",
    )
}
