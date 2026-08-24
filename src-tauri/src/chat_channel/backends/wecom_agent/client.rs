use std::time::Duration;

use serde::Deserialize;

use crate::chat_channel::error::ChatChannelError;

const GET_TOKEN_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/gettoken";
const SEND_MESSAGE_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/message/send";
const MAX_TEXT_BYTES: usize = 2048;

#[derive(Debug)]
pub struct AccessToken {
    pub value: String,
    pub expires_in: u64,
}

#[derive(Debug)]
pub struct SendReceipt {
    pub message_id: Option<String>,
}

#[derive(Debug)]
pub enum SendError {
    TokenInvalid { code: i64, message: String },
    Failed(ChatChannelError),
}

#[derive(Clone)]
pub struct WecomAgentClient {
    pub(super) client: reqwest::Client,
}

impl Default for WecomAgentClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl WecomAgentClient {
    pub async fn fetch_access_token(
        &self,
        corp_id: &str,
        app_secret: &str,
    ) -> Result<AccessToken, ChatChannelError> {
        let response = self
            .client
            .get(GET_TOKEN_URL)
            .query(&[("corpid", corp_id), ("corpsecret", app_secret)])
            .send()
            .await
            .map_err(|error| transport_error("access_token request failed", error))?;
        if !response.status().is_success() {
            return Err(ChatChannelError::ConnectionFailed(format!(
                "WeCom access_token request failed (HTTP {})",
                response.status()
            )));
        }
        let body: TokenResponse = response
            .json()
            .await
            .map_err(|error| transport_error("access_token response was invalid", error))?;
        if body.errcode != 0 {
            return Err(ChatChannelError::AuthenticationFailed(format!(
                "WeCom rejected the application credential (errcode {}, {})",
                body.errcode,
                provider_message(&body.errmsg)
            )));
        }
        let value = body
            .access_token
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ChatChannelError::ConnectionFailed(
                    "WeCom access_token response omitted access_token".to_string(),
                )
            })?;
        Ok(AccessToken {
            value,
            expires_in: body.expires_in.unwrap_or(7200).max(60),
        })
    }

    pub async fn send_text(
        &self,
        access_token: &str,
        user_id: &str,
        agent_id: i64,
        text: &str,
    ) -> Result<SendReceipt, SendError> {
        if text.as_bytes().len() > MAX_TEXT_BYTES {
            return Err(SendError::Failed(ChatChannelError::SendFailed(
                "WeCom text message exceeds 2048 bytes".to_string(),
            )));
        }
        self.send_payload(
            access_token,
            serde_json::json!({
                "touser": user_id,
                "msgtype": "text",
                "agentid": agent_id,
                "text": { "content": text },
                "enable_duplicate_check": 1,
                "duplicate_check_interval": 1800,
            }),
        )
        .await
    }

    pub(super) async fn send_payload(
        &self,
        access_token: &str,
        payload: serde_json::Value,
    ) -> Result<SendReceipt, SendError> {
        let response = self
            .client
            .post(SEND_MESSAGE_URL)
            .query(&[("access_token", access_token)])
            .json(&payload)
            .send()
            .await
            .map_err(|error| SendError::Failed(transport_error("message request failed", error)))?;
        if !response.status().is_success() {
            return Err(send_failed(&format!(
                "WeCom message request failed (HTTP {})",
                response.status()
            )));
        }
        let body: SendResponse = response.json().await.map_err(|error| {
            SendError::Failed(transport_error("message response was invalid", error))
        })?;
        provider_result(body.errcode, &body.errmsg, "message")?;
        validate_recipients(&body)?;
        Ok(SendReceipt {
            message_id: body.msg_id.filter(|value| !value.trim().is_empty()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    errcode: i64,
    #[serde(default)]
    errmsg: String,
    access_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SendResponse {
    #[serde(default)]
    errcode: i64,
    #[serde(default)]
    errmsg: String,
    #[serde(default, alias = "invaliduser")]
    invalid_user: Option<String>,
    #[serde(default, alias = "unlicenseduser")]
    unlicensed_user: Option<String>,
    #[serde(default, alias = "msgid")]
    msg_id: Option<String>,
}

pub(super) fn provider_result(code: i64, message: &str, operation: &str) -> Result<(), SendError> {
    if is_token_error(code) {
        return Err(SendError::TokenInvalid {
            code,
            message: provider_message(message).to_string(),
        });
    }
    if code != 0 {
        return Err(send_failed(&format!(
            "WeCom rejected {operation} (errcode {code}, {})",
            provider_message(message)
        )));
    }
    Ok(())
}

fn validate_recipients(body: &SendResponse) -> Result<(), SendError> {
    let invalid = recipient_count(body.invalid_user.as_deref());
    let unlicensed = recipient_count(body.unlicensed_user.as_deref());
    if invalid == 0 && unlicensed == 0 {
        return Ok(());
    }
    Err(send_failed(&format!(
        "WeCom did not deliver to all recipients (invalid {invalid}, unlicensed {unlicensed})"
    )))
}

pub(super) fn send_failed(message: &str) -> SendError {
    SendError::Failed(ChatChannelError::SendFailed(message.to_string()))
}

fn is_token_error(code: i64) -> bool {
    matches!(code, 40014 | 42001 | 42007 | 42009)
}

fn recipient_count(value: Option<&str>) -> usize {
    value
        .map(|value| {
            value
                .split('|')
                .filter(|part| !part.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

fn provider_message(value: &str) -> &str {
    if value.trim().is_empty() {
        "unknown provider error"
    } else {
        value
    }
}

pub(super) fn transport_error(context: &str, error: reqwest::Error) -> ChatChannelError {
    ChatChannelError::ConnectionFailed(format!("{context}: {}", error.without_url()))
}

impl From<SendError> for ChatChannelError {
    fn from(error: SendError) -> Self {
        match error {
            SendError::TokenInvalid { code, message } => Self::AuthenticationFailed(format!(
                "WeCom access_token remained invalid after refresh (errcode {code}, {message})"
            )),
            SendError::Failed(error) => error,
        }
    }
}
