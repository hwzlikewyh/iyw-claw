use reqwest::StatusCode;
use serde_json::Value;

use crate::chat_channel::error::ChatChannelError;

use super::super::types::{
    LarkRegion, ProviderCredentials, ProviderPoll, ProviderSession, ProviderStart,
};

const FEISHU_REGISTRATION_URL: &str = "https://accounts.feishu.cn/oauth/v1/app/registration";
const LARK_REGISTRATION_URL: &str = "https://accounts.larksuite.com/oauth/v1/app/registration";

pub async fn start(
    client: &reqwest::Client,
    region: LarkRegion,
) -> Result<ProviderStart, ChatChannelError> {
    let (_, initialized) = post_form(client, region, &[("action", "init")]).await?;
    ensure_no_error(&initialized)?;
    let (_, body) = post_form(
        client,
        region,
        &[
            ("action", "begin"),
            ("archetype", "PersonalAgent"),
            ("auth_method", "client_secret"),
            ("request_user_info", "open_id"),
        ],
    )
    .await?;
    ensure_no_error(&body)?;
    let device_code = required(&body, "device_code")?;
    let qr_content = verification_url(&body, region)
        .ok_or_else(|| protocol_error("verification_uri_complete"))?;
    Ok(ProviderStart {
        session: ProviderSession::Lark {
            device_code,
            region,
        },
        qr_content,
        expires_in_secs: first_u64(&body, &["expires_in"])
            .unwrap_or(900)
            .clamp(60, 7200),
        retry_after_ms: first_u64(&body, &["interval"]).unwrap_or(3).clamp(2, 30) * 1000,
    })
}

pub async fn poll(
    client: &reqwest::Client,
    region: LarkRegion,
    device_code: &str,
) -> Result<ProviderPoll, ChatChannelError> {
    let (_, body) = post_form(
        client,
        region,
        &[("action", "poll"), ("device_code", device_code)],
    )
    .await?;
    if let Some(error) = first_string(&body, &["error", "error_code"]) {
        return Ok(match error.as_str() {
            "authorization_pending" | "slow_down" => ProviderPoll::Waiting,
            "expired_token" | "invalid_grant" => ProviderPoll::Expired,
            "access_denied" => ProviderPoll::Denied("access_denied"),
            _ => return Err(protocol_error("provider_error")),
        });
    }
    let client_id = first_string(&body, &["client_id", "app_id"]);
    let client_secret = first_string(&body, &["client_secret", "app_secret"]);
    match (client_id, client_secret) {
        (Some(client_id), Some(token)) => Ok(ProviderPoll::Approved(ProviderCredentials {
            token,
            config_patch: serde_json::json!({
                "appId": client_id,
                "larkRegion": region.config_value(),
            }),
        })),
        _ => Ok(ProviderPoll::Waiting),
    }
}

async fn post_form(
    client: &reqwest::Client,
    region: LarkRegion,
    form: &[(&str, &str)],
) -> Result<(StatusCode, Value), ChatChannelError> {
    let response = client
        .post(registration_url(region))
        .form(form)
        .send()
        .await
        .map_err(network_error)?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|error| super::request_error("飞书/Lark", &error))?;
    if !status.is_success() && first_string(&body, &["error", "error_code"]).is_none() {
        return Err(protocol_error("http_error"));
    }
    Ok((status, body))
}

fn registration_url(region: LarkRegion) -> &'static str {
    match region {
        LarkRegion::Feishu => FEISHU_REGISTRATION_URL,
        LarkRegion::Lark => LARK_REGISTRATION_URL,
    }
}

fn ensure_no_error(body: &Value) -> Result<(), ChatChannelError> {
    if first_string(body, &["error", "error_code"]).is_some() {
        return Err(protocol_error("provider_error"));
    }
    Ok(())
}

fn verification_url(body: &Value, region: LarkRegion) -> Option<String> {
    if let Some(url) = first_string(body, &["verification_uri_complete", "verification_url"]) {
        return Some(url);
    }
    if let Some(user_code) = first_string(body, &["user_code"]) {
        let mut url = reqwest::Url::parse(match region {
            LarkRegion::Feishu => "https://open.feishu.cn/page/cli",
            LarkRegion::Lark => "https://open.larksuite.com/page/cli",
        })
        .ok()?;
        url.query_pairs_mut().append_pair("user_code", &user_code);
        return Some(url.to_string());
    }
    first_string(body, &["verification_uri"])
}

fn required(body: &Value, field: &str) -> Result<String, ChatChannelError> {
    first_string(body, &[field]).ok_or_else(|| protocol_error(field))
}

fn first_string(body: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        body.get(*field)
            .or_else(|| body.pointer(&format!("/data/{field}")))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn first_u64(body: &Value, fields: &[&str]) -> Option<u64> {
    fields.iter().find_map(|field| {
        body.get(*field)
            .or_else(|| body.pointer(&format!("/data/{field}")))
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
    })
}

fn protocol_error(field: &str) -> ChatChannelError {
    ChatChannelError::ConnectionFailed(format!("飞书扫码响应缺少或拒绝 {field}"))
}

fn network_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("飞书/Lark", &error)
}
