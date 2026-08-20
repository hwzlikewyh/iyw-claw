use serde::Deserialize;

use crate::chat_channel::error::ChatChannelError;

use super::super::types::{ProviderCredentials, ProviderPoll, ProviderSession, ProviderStart};

const GENERATE_URL: &str = "https://work.weixin.qq.com/ai/qc/generate";
const QUERY_URL: &str = "https://work.weixin.qq.com/ai/qc/query_result";
const SOURCE: &str = "wecom_cli_external";

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

#[derive(Deserialize)]
struct GenerateData {
    scode: Option<String>,
    auth_url: Option<String>,
}

#[derive(Deserialize)]
struct QueryData {
    status: Option<String>,
    bot_info: Option<BotInfo>,
}

#[derive(Deserialize)]
struct BotInfo {
    botid: Option<String>,
    secret: Option<String>,
}

pub async fn start(client: &reqwest::Client) -> Result<ProviderStart, ChatChannelError> {
    let response = client
        .get(GENERATE_URL)
        .query(&[("source", SOURCE), ("plat", platform_code())])
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?
        .json::<Envelope<GenerateData>>()
        .await
        .map_err(decode_error)?;
    let data = response.data.ok_or_else(protocol_error)?;
    let scode = required(data.scode, "scode")?;
    let qr_content = required(data.auth_url, "auth_url")?;
    Ok(ProviderStart {
        session: ProviderSession::WecomAiBot { scode },
        qr_content,
        expires_in_secs: 300,
        retry_after_ms: 3000,
    })
}

pub async fn poll(client: &reqwest::Client, scode: &str) -> Result<ProviderPoll, ChatChannelError> {
    let response = client
        .get(QUERY_URL)
        .query(&[("scode", scode)])
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?
        .json::<Envelope<QueryData>>()
        .await
        .map_err(decode_error)?;
    let Some(data) = response.data else {
        return Ok(ProviderPoll::Waiting);
    };
    if data.status.as_deref() != Some("success") {
        return Ok(ProviderPoll::Waiting);
    }
    credentials(data.bot_info)
}

fn credentials(bot_info: Option<BotInfo>) -> Result<ProviderPoll, ChatChannelError> {
    let bot = bot_info.ok_or_else(protocol_error)?;
    let bot_id = required(bot.botid, "botid")?;
    let token = required(bot.secret, "secret")?;
    Ok(ProviderPoll::Approved(ProviderCredentials {
        token,
        config_patch: serde_json::json!({
            "botId": bot_id,
            "defaultChatType": 1,
        }),
    }))
}

fn platform_code() -> &'static str {
    if cfg!(target_os = "macos") {
        "1"
    } else if cfg!(target_os = "windows") {
        "2"
    } else if cfg!(target_os = "linux") {
        "3"
    } else {
        "0"
    }
}

fn required(value: Option<String>, field: &str) -> Result<String, ChatChannelError> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChatChannelError::ConnectionFailed(format!("企微扫码响应缺少 {field}")))
}

fn protocol_error() -> ChatChannelError {
    ChatChannelError::ConnectionFailed("企微扫码响应格式无效".to_string())
}

fn network_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("企微", &error)
}

fn decode_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("企微", &error)
}
