use std::{future::Future, time::Duration};

use crate::acp::runtime_observation::RetryProgress;
use axum::http::{header::RETRY_AFTER, HeaderMap, Method, StatusCode};
use rand::Rng;
use reqwest::{Client, Response, Url};
use tokio::time::{sleep_until, timeout_at, Instant};

use super::{
    auth, diagnostics::CloudAttempt, error::GatewayError, limits::HEADER_TIMEOUT,
    retry_progress::RetryTurn, spool::UploadBody,
};

const MAX_RETRIES: u32 = 10;
const MAX_BACKOFF: Duration = Duration::from_secs(15);
const JITTER_MS: u64 = 400;
const BACKOFF_STEP_SECS: u64 = 2;

pub(super) struct CloudRequest<'a> {
    pub client: &'a Client,
    pub method: Method,
    pub target: Url,
    pub headers: HeaderMap,
    pub token: &'a str,
    pub content_length: u64,
    pub retry_turn: Option<RetryTurn>,
}

impl CloudRequest<'_> {
    pub(super) async fn send(
        &self,
        body: UploadBody,
        path: &str,
    ) -> Result<Response, GatewayError> {
        let deadline = Instant::now() + HEADER_TIMEOUT;
        let max_retries = if allows_retry(&self.method, path) {
            MAX_RETRIES
        } else {
            0
        };
        let mut previous_delay = Duration::ZERO;
        for attempt in 1..=max_retries {
            let response = self.send_once(body.open(), deadline).await?;
            let Some(delay) = retry_delay(&response, attempt, previous_delay) else {
                return Ok(response);
            };
            let wake = Instant::now() + delay;
            if wake >= deadline {
                return Ok(response);
            }
            previous_delay = delay;
            if let Some(turn) = &self.retry_turn {
                let progress = RetryProgress {
                    attempt: Some(attempt),
                    max_attempts: Some(MAX_RETRIES),
                    reason: Some(format!("HTTP {}", response.status().as_u16())),
                    retry_at: Some(chrono::Utc::now() + delay),
                };
                if !turn.notify(progress).await {
                    return Ok(response);
                }
            }
            tracing::warn!(target: "model_gateway", phase = "cloud_retry",
                status = response.status().as_u16(), retry = attempt,
                max_retries = MAX_RETRIES, delay_ms = delay.as_millis() as u64,
                "retrying cloud model request after upstream error");
            sleep_until(wake).await;
            if Instant::now() >= deadline {
                return Ok(response);
            }
            if let Some(turn) = &self.retry_turn {
                if !turn.active().await {
                    return Ok(response);
                }
            }
        }
        // 最后一次上传使用完即可释放暂存额度，无需持有到云端返回响应头。
        self.send_once(async move { body.open().await }, deadline)
            .await
    }

    async fn send_once(
        &self,
        upload: impl Future<Output = Result<reqwest::Body, GatewayError>>,
        deadline: Instant,
    ) -> Result<Response, GatewayError> {
        let diagnostic = CloudAttempt::new(&self.target);
        let sending = async {
            let body = upload.await?;
            auth::check(self.token)?;
            self.client
                .request(self.method.clone(), self.target.clone())
                .headers(self.headers.clone())
                .header("token", self.token)
                .header("x-iyw-native-vision", "true")
                .header("accept-encoding", "identity")
                .header("content-length", self.content_length)
                .body(body)
                .send()
                .await
                .map_err(|error| {
                    diagnostic.failed(&error);
                    GatewayError::cloud(error)
                })
        };
        let response = timeout_at(deadline, sending).await.map_err(|_| {
            diagnostic.timed_out();
            GatewayError {
                status: StatusCode::GATEWAY_TIMEOUT,
                code: "cloud_gateway_timeout",
                message: "Cloud model response headers timed out",
            }
        })??;
        diagnostic.responded(response.status().as_u16());
        Ok(response)
    }
}

fn allows_retry(method: &Method, path: &str) -> bool {
    let path = path.strip_prefix("/anthropic").unwrap_or(path);
    method == Method::POST
        && matches!(
            path,
            "/v1/responses"
                | "/v1/responses/compact"
                | "/v1/chat/completions"
                | "/v1/messages"
                | "/v1/messages/count_tokens"
        )
}

fn retry_delay(response: &Response, attempt: u32, previous: Duration) -> Option<Duration> {
    if !matches!(
        response.status(),
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    ) {
        return None;
    }
    let base = Duration::from_secs(1 + u64::from(attempt - 1) * BACKOFF_STEP_SECS);
    let jitter = Duration::from_millis(rand::thread_rng().gen_range(0..=JITTER_MS));
    let minimum = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(retry_after)
        .unwrap_or_default();
    if minimum > MAX_BACKOFF {
        return None;
    }
    let delay = base.max(minimum).saturating_add(jitter).min(MAX_BACKOFF);
    Some(delay.max(previous))
}

fn retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = chrono::DateTime::parse_from_rfc2822(value.trim()).ok()?;
    Some(
        (date.to_utc() - chrono::Utc::now())
            .to_std()
            .unwrap_or_default(),
    )
}
