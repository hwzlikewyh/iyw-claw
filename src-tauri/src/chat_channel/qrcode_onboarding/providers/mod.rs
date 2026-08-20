mod dingtalk;
mod lark;
mod wecom_ai_bot;

use std::sync::OnceLock;
use std::time::Duration;

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::ChannelType;

use super::types::{LarkRegion, ProviderPoll, ProviderSession, ProviderStart};

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn start(
    channel_type: ChannelType,
    region: LarkRegion,
    local_weixin_tokens: &[String],
) -> Result<ProviderStart, ChatChannelError> {
    match channel_type {
        ChannelType::Weixin => start_weixin(local_weixin_tokens).await,
        ChannelType::WecomAiBot => wecom_ai_bot::start(http_client()).await,
        ChannelType::Dingtalk => dingtalk::start(http_client()).await,
        ChannelType::Lark => lark::start(http_client(), region).await,
        _ => Err(ChatChannelError::ConfigurationInvalid(
            "该渠道不支持扫码接入".to_string(),
        )),
    }
}

pub async fn poll(
    session: &ProviderSession,
    verify_code: Option<&str>,
) -> Result<ProviderPoll, ChatChannelError> {
    match session {
        ProviderSession::Weixin { qrcode } => poll_weixin(qrcode, verify_code).await,
        ProviderSession::WecomAiBot { scode } => wecom_ai_bot::poll(http_client(), scode).await,
        ProviderSession::Dingtalk { device_code } => {
            dingtalk::poll(http_client(), device_code).await
        }
        ProviderSession::Lark {
            device_code,
            region,
        } => lark::poll(http_client(), *region, device_code).await,
    }
}

pub async fn finish(session: &ProviderSession) {
    if let ProviderSession::Weixin { qrcode } = session {
        crate::chat_channel::backends::weixin::weixin_forget_qrcode(qrcode).await;
    }
}

pub(super) fn request_error(provider: &str, error: &reqwest::Error) -> ChatChannelError {
    let detail = if error.is_timeout() {
        "request timed out".to_string()
    } else if let Some(status) = error.status() {
        format!("HTTP {status}")
    } else if error.is_connect() {
        "connection failed".to_string()
    } else if error.is_decode() {
        "response decode failed".to_string()
    } else {
        "request failed".to_string()
    };
    ChatChannelError::ConnectionFailed(format!("{provider}扫码{detail}"))
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default()
    })
}

async fn start_weixin(tokens: &[String]) -> Result<ProviderStart, ChatChannelError> {
    let info = crate::chat_channel::backends::weixin::weixin_get_qrcode(tokens).await?;
    Ok(ProviderStart {
        session: ProviderSession::Weixin {
            qrcode: info.qrcode_id,
        },
        qr_content: info.qrcode_img_content,
        expires_in_secs: 300,
        retry_after_ms: 1000,
    })
}

async fn poll_weixin(
    qrcode: &str,
    verify_code: Option<&str>,
) -> Result<ProviderPoll, ChatChannelError> {
    let result =
        crate::chat_channel::backends::weixin::weixin_check_qrcode(qrcode, verify_code).await?;
    match result.status.as_str() {
        "confirmed" => {
            let token = result.bot_token.ok_or_else(|| {
                ChatChannelError::AuthenticationFailed("微信授权响应缺少凭据".to_string())
            })?;
            Ok(ProviderPoll::Approved(super::types::ProviderCredentials {
                token,
                config_patch: serde_json::json!({
                    "baseUrl": result.base_url.unwrap_or_else(|| {
                        "https://ilinkai.weixin.qq.com".to_string()
                    })
                }),
            }))
        }
        "scaned" | "scaned_but_redirect" => Ok(ProviderPoll::Scanned),
        "need_verifycode" => Ok(ProviderPoll::VerificationRequired),
        "expired" => Ok(ProviderPoll::Expired),
        "binded_redirect" => Ok(ProviderPoll::Denied("already_bound")),
        "verify_code_blocked" => Ok(ProviderPoll::Denied("verify_code_blocked")),
        _ => Ok(ProviderPoll::Waiting),
    }
}
