//! WeCom self-built application backend.

mod client;
pub mod crypto;

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use self::client::{SendError, WecomAgentClient};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;

const TOKEN_REFRESH_MARGIN_SECS: u64 = 300;

#[derive(Debug)]
struct CachedToken {
    value: String,
    refresh_at: Instant,
}

#[derive(Clone)]
pub struct WecomAgentBackend {
    channel_id: i32,
    config: WecomAgentConfig,
    secrets: WecomAgentSecrets,
    agent_id: i64,
    client: WecomAgentClient,
    token: Arc<Mutex<Option<CachedToken>>>,
    status: Arc<Mutex<ChannelConnectionStatus>>,
}

impl WecomAgentBackend {
    pub fn new(
        channel_id: i32,
        config: WecomAgentConfig,
        secrets: WecomAgentSecrets,
    ) -> Result<Self, ChatChannelError> {
        validate_config(&config, &secrets)?;
        let agent_id = config.agent_id.parse::<i64>().map_err(|_| {
            ChatChannelError::ConfigurationInvalid(
                "WeCom AgentID must be a positive integer".to_string(),
            )
        })?;
        if agent_id <= 0 {
            return Err(ChatChannelError::ConfigurationInvalid(
                "WeCom AgentID must be a positive integer".to_string(),
            ));
        }
        Ok(Self {
            channel_id,
            config,
            secrets,
            agent_id,
            client: WecomAgentClient::default(),
            token: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(ChannelConnectionStatus::Disconnected)),
        })
    }

    async fn access_token(
        &self,
        force_refresh: bool,
        rejected_token: Option<&str>,
    ) -> Result<String, ChatChannelError> {
        let mut cache = self.token.lock().await;
        if let Some(cached) = cache.as_ref().filter(|cached| {
            Instant::now() < cached.refresh_at
                && (!force_refresh
                    || rejected_token.is_some_and(|value| value != cached.value.as_str()))
        }) {
            return Ok(cached.value.clone());
        }
        let token = self
            .client
            .fetch_access_token(&self.config.corp_id, &self.secrets.app_secret)
            .await?;
        let refresh_after = token
            .expires_in
            .saturating_sub(TOKEN_REFRESH_MARGIN_SECS)
            .max(30);
        let value = token.value.clone();
        *cache = Some(CachedToken {
            value: token.value,
            refresh_at: Instant::now() + Duration::from_secs(refresh_after),
        });
        Ok(value)
    }

    async fn send_text_to(
        &self,
        user_id: &str,
        text: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        let user_id = user_id.trim();
        if user_id.is_empty() {
            return Err(ChatChannelError::ConfigurationInvalid(
                "WeCom target UserID is missing".to_string(),
            ));
        }
        let token = self.access_token(false, None).await?;
        let first = self
            .client
            .send_text(&token, user_id, self.agent_id, text)
            .await;
        let receipt = match first {
            Ok(receipt) => receipt,
            Err(SendError::TokenInvalid { code, message }) => {
                tracing::warn!(
                    channel_id = self.channel_id,
                    provider_code = code,
                    provider_message = message,
                    "[WeCom Agent] access_token expired; refreshing once"
                );
                let refreshed = self.access_token(true, Some(&token)).await?;
                self.client
                    .send_text(&refreshed, user_id, self.agent_id, text)
                    .await
                    .map_err(map_send_error)?
            }
            Err(error) => return Err(map_send_error(error)),
        };
        Ok(SentMessageId(receipt.message_id.unwrap_or_else(|| {
            format!("wecom-agent-{}", uuid::Uuid::new_v4())
        })))
    }
}

#[async_trait]
impl ChatChannelBackend for WecomAgentBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::WecomAgent
    }

    async fn start(
        &self,
        _command_tx: mpsc::Sender<IncomingCommand>,
        _runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        *self.status.lock().await = ChannelConnectionStatus::Connecting;
        if let Err(error) = self.access_token(false, None).await {
            *self.status.lock().await = ChannelConnectionStatus::Error;
            tracing::warn!(
                channel_id = self.channel_id,
                channel_type = "wecom_agent",
                generation,
                stage = "credential_validation",
                error_category = error.category(),
                error = %error,
                "[WeCom Agent] startup failed"
            );
            return Err(error);
        }
        *self.status.lock().await = ChannelConnectionStatus::Connected;
        tracing::info!(
            channel_id = self.channel_id,
            generation,
            "[WeCom Agent] outbound credential ready; callback readiness is tracked separately"
        );
        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        *self.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.status.lock().await
    }

    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        self.send_text_to(&self.config.default_user_id, text).await
    }

    async fn send_rich_message(
        &self,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_message(&message.to_plain_text()).await
    }

    async fn send_rich_message_to(
        &self,
        message: &RichMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let user_id = target.chat_id.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("WeCom target UserID is missing".to_string())
        })?;
        self.send_text_to(user_id, &message.to_plain_text()).await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        self.access_token(true, None).await.map(|_| ())
    }
}

fn validate_config(
    config: &WecomAgentConfig,
    secrets: &WecomAgentSecrets,
) -> Result<(), ChatChannelError> {
    if config.corp_id.trim().is_empty() || config.callback_path.trim().is_empty() {
        return Err(ChatChannelError::ConfigurationInvalid(
            "WeCom CorpID and callback path are required".to_string(),
        ));
    }
    if config.callback_path.len() < 16
        || config.callback_path.len() > 128
        || !config
            .callback_path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ChatChannelError::ConfigurationInvalid(
            "WeCom callback path is invalid".to_string(),
        ));
    }
    let url = reqwest::Url::parse(&config.external_base_url).map_err(|_| {
        ChatChannelError::ConfigurationInvalid(
            "WeCom external callback URL must be valid HTTPS".to_string(),
        )
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ChatChannelError::ConfigurationInvalid(
            "WeCom external callback URL must be valid HTTPS".to_string(),
        ));
    }
    crypto::encrypt(&secrets.encoding_aes_key, "", config.corp_id.trim())?;
    Ok(())
}

pub(crate) fn ensure_ready_config(config: &serde_json::Value) -> Result<(), ChatChannelError> {
    let setup_ready = config
        .get("setup_state")
        .and_then(serde_json::Value::as_str)
        == Some("ready");
    let callback_verified = config
        .get("callback_verified_at")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if setup_ready && callback_verified {
        return Ok(());
    }
    Err(ChatChannelError::ConfigurationInvalid(
        "企业微信自建应用尚未完成回调验证".to_string(),
    ))
}

pub(crate) fn prepare_new_config(config_json: &str) -> Result<String, ChatChannelError> {
    let mut config = serde_json::from_str::<serde_json::Value>(config_json)
        .map_err(|_| ChatChannelError::ConfigurationInvalid("WeCom config is invalid".into()))?;
    let map = config.as_object_mut().ok_or_else(|| {
        ChatChannelError::ConfigurationInvalid("WeCom config must be an object".into())
    })?;
    map.remove("callback_verified_at");
    if map.get("setup_state").and_then(serde_json::Value::as_str) == Some("ready") {
        map.insert(
            "setup_state".to_string(),
            serde_json::Value::String("pending_callback".to_string()),
        );
    }
    serde_json::to_string(&config)
        .map_err(|_| ChatChannelError::ConfigurationInvalid("WeCom config is invalid".into()))
}

#[cfg(test)]
#[path = "wecom_agent_tests.rs"]
mod tests;

fn map_send_error(error: SendError) -> ChatChannelError {
    match error {
        SendError::TokenInvalid { code, message } => {
            ChatChannelError::AuthenticationFailed(format!(
                "WeCom access_token remained invalid after refresh (errcode {code}, {message})"
            ))
        }
        SendError::Failed(error) => error,
    }
}
