use std::{collections::HashMap, future::Future, sync::Arc, time::Instant};

use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio::sync::{Mutex, OnceCell, Semaphore};
use tokio_util::sync::CancellationToken;

use super::{input::PreparedImage, limits::MAX_CONCURRENT_EXECUTIONS};

type ImageCell = Arc<OnceCell<Result<PreparedImage, ErrorData>>>;
type UploadCell = Arc<OnceCell<Result<String, ErrorData>>>;

pub(in crate::acp::builtin_mcp) struct ImageContext {
    pub catalog: OnceCell<Result<Arc<Vec<Value>>, ErrorData>>,
    pub refreshed_catalog: OnceCell<Result<Arc<Vec<Value>>, ErrorData>>,
    pub inputs: Semaphore,
    pub executions: Semaphore,
    pub scope: String,
    request_cancel: CancellationToken,
    session_cancel: CancellationToken,
    sources: Mutex<HashMap<String, ImageCell>>,
    uploads: Mutex<HashMap<String, UploadCell>>,
}

impl ImageContext {
    pub fn new(
        scope: String,
        request_cancel: CancellationToken,
        session_cancel: CancellationToken,
    ) -> Self {
        Self {
            catalog: OnceCell::new(),
            refreshed_catalog: OnceCell::new(),
            inputs: Semaphore::new(MAX_CONCURRENT_EXECUTIONS),
            executions: Semaphore::new(MAX_CONCURRENT_EXECUTIONS),
            scope,
            request_cancel,
            session_cancel,
            sources: Mutex::new(HashMap::new()),
            uploads: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run<T>(
        &self,
        stage: &str,
        operation: impl Future<Output = Result<T, ErrorData>>,
    ) -> Result<T, ErrorData> {
        tokio::select! {
            biased;
            _ = self.request_cancel.cancelled() => Err(cancelled(stage)),
            _ = self.session_cancel.cancelled() => Err(cancelled(stage)),
            result = operation => result,
        }
    }

    pub(super) async fn prepare(
        &self,
        key: String,
        operation: impl Future<Output = Result<PreparedImage, ErrorData>>,
    ) -> Result<PreparedImage, ErrorData> {
        let cell = self.sources.lock().await.entry(key).or_default().clone();
        tracing::debug!(
            cache_hit = cell.initialized(),
            "[iyw-image] input reuse lookup"
        );
        self.run("input", async {
            cell.get_or_init(|| async {
                let started = Instant::now();
                let _permit = self
                    .inputs
                    .acquire()
                    .await
                    .map_err(|_| cancelled("input"))?;
                let result = operation.await;
                tracing::info!(
                    duration_ms = started.elapsed().as_millis(),
                    succeeded = result.is_ok(),
                    "[iyw-image] unique input preparation completed"
                );
                result
            })
            .await
            .clone()
        })
        .await
    }

    pub async fn upload(
        &self,
        key: String,
        operation: impl Future<Output = Result<String, ErrorData>>,
    ) -> Result<String, ErrorData> {
        let cell = self.uploads.lock().await.entry(key).or_default().clone();
        tracing::debug!(
            cache_hit = cell.initialized(),
            "[iyw-image] upload reuse lookup"
        );
        self.run("input", async {
            cell.get_or_init(|| operation).await.clone()
        })
        .await
    }
}

fn cancelled(stage: &str) -> ErrorData {
    let status = if matches!(stage, "input" | "catalog" | "queue") {
        "not_started"
    } else {
        "unknown"
    };
    tracing::info!(
        stage,
        execution_status = status,
        "[iyw-image] operation cancelled"
    );
    ErrorData::internal_error(
        "Image request cancelled; preserve known task IDs and completed images before retrying",
        Some(json!({"code": "image_request_cancelled", "stage": stage,
            "execution_status": status, "retryable": false})),
    )
}
