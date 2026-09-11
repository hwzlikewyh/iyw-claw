use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::{timeout_at, Instant};

use super::{BrowserError, CdpObserverHandle, CdpRequest};
use crate::browser::cdp_errors::{timeout, unavailable};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

impl CdpObserverHandle {
    pub async fn stop(&self) {
        self.cancellation.cancel();
        if let Some(mut task) = self.task.lock().await.take() {
            if tokio::time::timeout(super::OBSERVER_CLOSE_TIMEOUT, &mut task)
                .await
                .is_err()
            {
                tracing::warn!(target: "iyw_claw_browser",
                    "Browser event observer stop timed out; aborting the observer task");
                task.abort();
                let _ = task.await;
            }
        }
    }

    pub async fn call(
        &self,
        method: &str,
        params: Value,
        session_id: Option<String>,
    ) -> Result<Value, BrowserError> {
        let (response, result) = oneshot::channel();
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let permit = timeout_at(deadline, self.commands.reserve())
            .await
            .map_err(|_| timeout())?
            .map_err(|_| unavailable())?;
        permit.send(CdpRequest {
            method: method.to_string(),
            params,
            session_id,
            response,
        });
        timeout_at(deadline, result)
            .await
            .map_err(|_| timeout().effect_may_have_occurred(true).retryable(false))?
            .map_err(|_| unavailable().effect_may_have_occurred(true))?
    }
}
