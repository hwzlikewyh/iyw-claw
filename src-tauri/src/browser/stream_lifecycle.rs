use std::sync::Arc;

use tokio::task::JoinHandle;

use super::error::{BrowserError, BrowserErrorCode};
use super::stream::StreamSubscription;
use super::stream_task::{self, StreamTaskContext};
use super::types::BrowserFrameSubscriptionStatus;

const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) fn spawn_stream_task(context: StreamTaskContext) -> JoinHandle<()> {
    let status = Arc::clone(&context.status);
    tokio::spawn(async move {
        if let Err(error) = stream_task::run(context).await {
            tracing::warn!(
                target: "iyw_claw_browser",
                error_code = ?error.code,
                "browser frame stream stopped"
            );
        }
        *status.write().await = BrowserFrameSubscriptionStatus::Disconnected;
    })
}

pub(super) async fn stop_entries(entries: Vec<StreamSubscription>) {
    futures_util::future::join_all(entries.into_iter().map(stop_entry)).await;
}

pub(super) async fn stop_entry(mut entry: StreamSubscription) {
    entry.cancellation.cancel();
    if tokio::time::timeout(STOP_TIMEOUT, &mut entry.task)
        .await
        .is_err()
    {
        entry.task.abort();
        let _ = entry.task.await;
    }
}

pub(super) fn disconnected() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserStreamDisconnected,
        "The browser frame stream is disconnected",
    )
    .retryable(true)
}
