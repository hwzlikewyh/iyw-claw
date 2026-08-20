use serde::Deserialize;

use crate::chat_channel::error::ChatChannelError;

use super::super::types::{ProviderCredentials, ProviderPoll, ProviderSession, ProviderStart};

const BASE_URL: &str = "https://oapi.dingtalk.com";
const SOURCE: &str = "openClaw";

#[derive(Deserialize)]
struct RegistrationResponse {
    #[serde(default)]
    errcode: Option<i64>,
    nonce: Option<String>,
    device_code: Option<String>,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    status: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

pub async fn start(client: &reqwest::Client) -> Result<ProviderStart, ChatChannelError> {
    let initialized = post(
        client,
        "/app/registration/init",
        serde_json::json!({
            "source": SOURCE,
        }),
    )
    .await?;
    let nonce = required(initialized.nonce, "nonce")?;
    let begun = post(
        client,
        "/app/registration/begin",
        serde_json::json!({
            "nonce": nonce,
        }),
    )
    .await?;
    Ok(ProviderStart {
        session: ProviderSession::Dingtalk {
            device_code: required(begun.device_code, "device_code")?,
        },
        qr_content: required(begun.verification_uri_complete, "verification_uri_complete")?,
        expires_in_secs: begun.expires_in.unwrap_or(7200).clamp(60, 7200),
        retry_after_ms: begun.interval.unwrap_or(3).clamp(2, 30) * 1000,
    })
}

pub async fn poll(
    client: &reqwest::Client,
    device_code: &str,
) -> Result<ProviderPoll, ChatChannelError> {
    let result = post(
        client,
        "/app/registration/poll",
        serde_json::json!({
            "device_code": device_code,
        }),
    )
    .await?;
    match result
        .status
        .as_deref()
        .unwrap_or("WAITING")
        .to_ascii_uppercase()
        .as_str()
    {
        "WAITING" => Ok(ProviderPoll::Waiting),
        "SUCCESS" => approved(result),
        "EXPIRED" => Ok(ProviderPoll::Expired),
        "FAIL" => Ok(ProviderPoll::Denied("provider_denied")),
        _ => Ok(ProviderPoll::Waiting),
    }
}

async fn post(
    client: &reqwest::Client,
    path: &str,
    body: serde_json::Value,
) -> Result<RegistrationResponse, ChatChannelError> {
    let result = client
        .post(format!("{BASE_URL}{path}"))
        .json(&body)
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?
        .json::<RegistrationResponse>()
        .await
        .map_err(decode_error)?;
    if result.errcode.is_some_and(|code| code != 0) {
        return Err(ChatChannelError::ConnectionFailed(
            "钉钉扫码服务拒绝请求".to_string(),
        ));
    }
    Ok(result)
}

fn approved(result: RegistrationResponse) -> Result<ProviderPoll, ChatChannelError> {
    let client_id = required(result.client_id, "client_id")?;
    let token = required(result.client_secret, "client_secret")?;
    Ok(ProviderPoll::Approved(ProviderCredentials {
        token,
        config_patch: serde_json::json!({ "clientId": client_id }),
    }))
}

fn required(value: Option<String>, field: &str) -> Result<String, ChatChannelError> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChatChannelError::ConnectionFailed(format!("钉钉扫码响应缺少 {field}")))
}

fn network_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("钉钉", &error)
}

fn decode_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("钉钉", &error)
}
