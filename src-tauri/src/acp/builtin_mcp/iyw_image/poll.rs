use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{IywGatewayService, WaitOptions};

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const QUERY_RETRY_DELAYS: [u64; 2] = [1, 2];

pub(super) async fn pause(
    service: &IywGatewayService,
    wait: &WaitOptions,
    deadline: Instant,
) -> bool {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return false;
    }
    let delay = Duration::from_secs_f64(wait.poll_interval_seconds).min(remaining);
    service
        .image_io("poll_wait", async {
            tokio::time::sleep(delay).await;
            Ok(Instant::now() < deadline)
        })
        .await
        .unwrap_or(false)
}

pub(super) async fn query(
    service: &IywGatewayService,
    (prefix, path): (&str, &str),
    (body, deadline): (Value, Instant),
) -> Result<Value, rmcp::ErrorData> {
    for attempt in 0..=QUERY_RETRY_DELAYS.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(rmcp::ErrorData::internal_error(
                "Image query wait deadline reached",
                Some(json!({"stage": "query", "execution_status": "unknown", "retryable": false})),
            ));
        }
        let mut query_service = service.clone();
        query_service.image_timeout = Some(remaining.min(QUERY_TIMEOUT));
        let result = query_service.post_gateway(prefix, path, body.clone()).await;
        let delay = QUERY_RETRY_DELAYS.get(attempt).copied();
        if !result.as_ref().err().is_some_and(transient) || delay.is_none() {
            return result;
        }
        let delay = Duration::from_secs(delay.expect("retry delay checked"));
        if delay >= deadline.saturating_duration_since(Instant::now()) {
            return result;
        }
        tracing::warn!(
            retry_index = attempt + 1,
            "[iyw-image] retrying original read-only task query"
        );
        service
            .image_io("poll_wait", async {
                tokio::time::sleep(delay).await;
                Ok(())
            })
            .await?;
    }
    unreachable!("the final query attempt always returns")
}

fn transient(error: &rmcp::ErrorData) -> bool {
    let Some(data) = &error.data else {
        return false;
    };
    if data.get("code").and_then(Value::as_str) == Some("image_request_cancelled") {
        return false;
    }
    data.get("timeout").and_then(Value::as_bool) == Some(true)
        || data.get("connect").and_then(Value::as_bool) == Some(true)
        || matches!(data.get("status").and_then(Value::as_u64), Some(429 | 503))
}
