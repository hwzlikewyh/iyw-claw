use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{json, Value};

use crate::app_error::AppCommandError;

const MAX_ERROR_DETAIL_CHARS: usize = 500;

#[derive(Debug, Clone)]
pub(crate) struct ModelGatewayChatConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

pub(crate) struct StructuredChatRequest<'a> {
    pub system_prompt: &'a str,
    pub user_content: String,
    pub json_schema: Value,
    pub max_tokens: u32,
    pub operation: &'static str,
}

pub(crate) async fn call_structured(
    config: &ModelGatewayChatConfig,
    request: StructuredChatRequest<'_>,
) -> Result<String, AppCommandError> {
    let operation = request.operation;
    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
        .map_err(|error| request_error(operation, "HTTP client build failed", error))?;
    let response = client
        .post(&config.api_url)
        .bearer_auth(&config.api_key)
        .json(&json!({
            "model": config.model,
            "temperature": 0,
            "max_tokens": request.max_tokens,
            "response_format": { "type": "json_schema", "json_schema": request.json_schema },
            "messages": [
                { "role": "system", "content": request.system_prompt },
                { "role": "user", "content": request.user_content }
            ]
        }))
        .send()
        .await
        .map_err(|error| request_error(operation, "request failed", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| request_error(operation, "response read failed", error))?;
    if !status.is_success() {
        return Err(status_error(operation, status, &body));
    }
    response_content(operation, &body)
}

fn response_content(operation: &str, body: &str) -> Result<String, AppCommandError> {
    let root: Value = serde_json::from_str(body).map_err(|error| {
        AppCommandError::configuration_invalid(format!("{operation} response is not JSON"))
            .with_detail(error.to_string())
    })?;
    root.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            let refusal = root
                .pointer("/choices/0/message/refusal")
                .and_then(Value::as_str)
                .unwrap_or("missing message content");
            AppCommandError::configuration_invalid(format!("{operation} response has no content"))
                .with_detail(refusal)
        })
}

fn request_error(operation: &str, stage: &str, error: impl std::fmt::Display) -> AppCommandError {
    AppCommandError::network(format!("{operation} {stage}")).with_detail(error.to_string())
}

fn status_error(operation: &str, status: StatusCode, body: &str) -> AppCommandError {
    let detail = body
        .chars()
        .take(MAX_ERROR_DETAIL_CHARS)
        .collect::<String>();
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            AppCommandError::authentication_failed(format!("{operation} authentication failed"))
                .with_detail(detail)
        }
        StatusCode::TOO_MANY_REQUESTS => {
            AppCommandError::network(format!("{operation} rate limited")).with_detail(detail)
        }
        _ => AppCommandError::network(format!("{operation} returned HTTP {status}"))
            .with_detail(detail),
    }
}
