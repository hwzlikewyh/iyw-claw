use std::time::Instant;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::super::agent_tool_cancellation::cancelled_error;
use super::super::{BrowserError, BrowserErrorCode};

pub(super) struct Rpc {
    pub method: &'static str,
    pub params: Value,
    pub session: Option<String>,
}

pub(super) struct Request {
    pub rpc: Rpc,
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub reply: oneshot::Sender<Result<Value, BrowserError>>,
}

pub(super) struct Operation<'a> {
    pub cancellation: &'a CancellationToken,
    pub deadline: Instant,
}

#[derive(Debug)]
pub(super) struct Worker {
    sender: mpsc::Sender<Request>,
    stop: CancellationToken,
}

impl Worker {
    pub fn spawn() -> Result<Self, BrowserError> {
        if !super::engine::SUPPORTED {
            return Err(unavailable(
                "Embedded Obscura is unavailable on this target",
            ));
        }
        let (sender, receiver) = mpsc::channel(1);
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("iyw-obscura".into())
            .spawn(move || super::engine::run(receiver, stopped))
            .map_err(|_| unavailable("Could not start the embedded Obscura worker"))?;
        Ok(Self { sender, stop })
    }

    pub async fn call(&self, rpc: Rpc, operation: &Operation<'_>) -> Result<Value, BrowserError> {
        let (reply, result) = oneshot::channel();
        let request = Request {
            rpc,
            deadline: operation.deadline,
            cancellation: operation.cancellation.clone(),
            reply,
        };
        tokio::select! {
            biased;
            _ = operation.cancellation.cancelled() => Err(cancelled_error()),
            _ = tokio::time::sleep_until(operation.deadline.into()) => Err(timeout()),
            result = async {
                self.sender.send(request).await.map_err(|_| unavailable("The embedded Obscura worker stopped"))?;
                result.await.map_err(|_| unavailable("The embedded Obscura worker stopped"))?
            } => result,
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub(super) fn unavailable(message: &str) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserRuntimeUnavailable, message)
}

pub(super) fn timeout() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserOperationTimeout,
        "The embedded Obscura operation timed out",
    )
}
