use std::time::Duration;

use reqwest::{Client, RequestBuilder};
use serde_json::{json, Value};

use super::{protocol, response_code, ChatChannelError};

pub(super) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(35);
pub(super) const SESSION_PAUSE: Duration = Duration::from_secs(60 * 60);

pub(super) fn request(client: &Client, base_url: &str, token: &str) -> RequestBuilder {
    client
        .post(format!("{base_url}/ilink/bot/getupdates"))
        .headers(protocol::post_headers(Some(token)))
}

pub(super) async fn fetch(
    request: RequestBuilder,
    cursor: &str,
    timeout: Duration,
) -> Result<Option<Value>, ChatChannelError> {
    let result = request
        .timeout(timeout)
        .json(&json!({ "get_updates_buf": cursor, "base_info": protocol::base_info() }))
        .send()
        .await;
    let response = match result {
        Err(error) if error.is_timeout() && !error.is_connect() => return Ok(None),
        result => result
            .map_err(|error| ChatChannelError::ConnectionFailed(error.without_url().to_string()))?,
    };
    let status = response.status();
    if !status.is_success() {
        return Err(ChatChannelError::ConnectionFailed(format!(
            "Weixin polling HTTP {status}"
        )));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| ChatChannelError::ConnectionFailed("Invalid Weixin getupdates JSON".into()))?;
    validate(&body)?;
    Ok(Some(body))
}

fn validate(body: &Value) -> Result<(), ChatChannelError> {
    if !body.is_object()
        || ["ret", "errcode"]
            .iter()
            .filter_map(|key| body.get(*key))
            .any(|code| !code.is_i64())
    {
        return Err(ChatChannelError::ConnectionFailed(
            "Invalid Weixin getupdates response".into(),
        ));
    }
    if body["ret"].as_i64() == Some(-14) || body["errcode"].as_i64() == Some(-14) {
        return Err(ChatChannelError::AuthenticationFailed(
            "WEIXIN_ACCOUNT_SESSION_UNAVAILABLE".into(),
        ));
    }
    if let Some(code) = response_code(body).filter(|code| *code != 0) {
        return Err(ChatChannelError::ConnectionFailed(format!(
            "Weixin polling provider code {code}"
        )));
    }
    Ok(())
}

pub(super) fn next_timeout(body: &Value, current: Duration) -> Duration {
    body["longpolling_timeout_ms"]
        .as_u64()
        .filter(|ms| (1..=u64::from(u32::MAX)).contains(ms))
        .map(Duration::from_millis)
        .unwrap_or(current)
}
