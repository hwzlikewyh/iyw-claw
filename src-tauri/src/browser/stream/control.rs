use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::{timeout_at, Instant};

use super::{disconnected, BrowserError, BrowserGenerations, BrowserStreamRegistry, StreamControl};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

impl BrowserStreamRegistry {
    pub async fn acknowledge(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
        seq: u64,
    ) -> Result<(), BrowserError> {
        let control = self.control_for(subscription_id, generations).await?;
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        let permit = timeout_at(deadline, control.reserve())
            .await
            .map_err(|_| disconnected())?
            .map_err(|_| disconnected())?;
        let (response, result) = oneshot::channel();
        permit.send(StreamControl::Ack { seq, response });
        await_control(result, deadline).await
    }

    pub async fn input(
        &self,
        subscription_id: &str,
        generations: &BrowserGenerations,
        messages: Vec<Value>,
    ) -> Result<(), BrowserError> {
        let control = self.control_for(subscription_id, generations).await?;
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        let permit = timeout_at(deadline, control.reserve())
            .await
            .map_err(|_| disconnected())?
            .map_err(|_| disconnected())?;
        let (response, result) = oneshot::channel();
        permit.send(StreamControl::Input { messages, response });
        await_control(result, deadline)
            .await
            .map_err(|error| error.effect_may_have_occurred(true).retryable(false))
    }
}

async fn await_control(
    response: oneshot::Receiver<Result<(), BrowserError>>,
    deadline: Instant,
) -> Result<(), BrowserError> {
    timeout_at(deadline, response)
        .await
        .map_err(|_| disconnected())?
        .map_err(|_| disconnected())?
}
