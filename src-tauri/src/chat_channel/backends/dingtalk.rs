//! DingTalk Stream Mode backend.

mod protocol;
mod runtime;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::ACCEPT;
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;

const GATEWAY_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";
const BOT_TOPIC: &str = "/v1.0/im/bot/messages/get";

#[derive(Debug, Deserialize)]
struct GatewayData {
    endpoint: String,
    ticket: String,
}

#[derive(Debug, Deserialize)]
struct GatewayEnvelope {
    #[serde(default)]
    code: Option<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
    data: Option<GatewayData>,
}

#[derive(Clone)]
pub struct DingtalkBackend {
    pub(super) channel_id: i32,
    config: DingtalkConfig,
    client_secret: String,
    pub(super) client: reqwest::Client,
    pub(super) status: Arc<Mutex<ChannelConnectionStatus>>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl DingtalkBackend {
    pub fn new(channel_id: i32, config: DingtalkConfig, client_secret: String) -> Self {
        Self {
            channel_id,
            config,
            client_secret,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            status: Arc::new(Mutex::new(ChannelConnectionStatus::Disconnected)),
            shutdown_tx: Arc::new(Mutex::new(None)),
            task: Arc::new(Mutex::new(None)),
        }
    }

    async fn register_connection(&self) -> Result<GatewayData, ChatChannelError> {
        let response = self
            .client
            .post(GATEWAY_URL)
            .header(ACCEPT, "application/json")
            .json(&serde_json::json!({
                "clientId": self.config.client_id,
                "clientSecret": self.client_secret,
                "subscriptions": [{ "type": "CALLBACK", "topic": BOT_TOPIC }],
                "ua": "iyw-claw",
            }))
            .send()
            .await
            .map_err(|error| ChatChannelError::ConnectionFailed(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ChatChannelError::AuthenticationFailed(format!(
                "DingTalk gateway rejected credentials (HTTP {status})"
            )));
        }
        let gateway: GatewayEnvelope = response
            .json()
            .await
            .map_err(|error| ChatChannelError::ConnectionFailed(error.to_string()))?;
        if let Some(code) = gateway.code.as_ref().filter(|code| !provider_code_ok(code)) {
            return Err(ChatChannelError::AuthenticationFailed(format!(
                "DingTalk gateway rejected credentials (provider code {}, message {})",
                provider_code(code),
                gateway.message.as_deref().unwrap_or("unknown error")
            )));
        }
        let gateway = gateway.data.ok_or_else(|| {
            ChatChannelError::ConnectionFailed("DingTalk gateway response omitted data".into())
        })?;
        if gateway.endpoint.trim().is_empty() || gateway.ticket.trim().is_empty() {
            return Err(ChatChannelError::ConnectionFailed(
                "DingTalk gateway response omitted endpoint or ticket".into(),
            ));
        }
        Ok(gateway)
    }

    pub(super) async fn open_stream(&self) -> Result<runtime::DingTalkSocket, ChatChannelError> {
        let gateway = self.register_connection().await?;
        let separator = if gateway.endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        let url = format!(
            "{}{separator}ticket={}",
            gateway.endpoint,
            urlencoding::encode(&gateway.ticket)
        );
        let mut stream = tokio_tungstenite::connect_async(url)
            .await
            .map(|(stream, _)| stream)
            .map_err(|error| ChatChannelError::ConnectionFailed(redact_transport_error(&error)))?;
        runtime::await_registered(&mut stream, self.channel_id).await?;
        Ok(stream)
    }

    async fn send_text_to(
        &self,
        text: &str,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let payload = target.provider_payload.as_ref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("DingTalk reply context is missing".into())
        })?;
        let webhook = protocol::string_field(payload, "session_webhook")?;
        if protocol::webhook_expired(payload) {
            return Err(ChatChannelError::SendFailed(
                "DingTalk session webhook expired; wait for a new inbound message".into(),
            ));
        }
        let response = self
            .client
            .post(webhook)
            .json(&serde_json::json!({
                "msgtype": "markdown",
                "markdown": { "title": "iyw-claw", "text": text },
            }))
            .send()
            .await
            .map_err(|error| ChatChannelError::SendFailed(redact_transport_error(&error)))?;
        if !response.status().is_success() {
            return Err(ChatChannelError::SendFailed(format!(
                "DingTalk reply failed (HTTP {})",
                response.status()
            )));
        }
        Ok(SentMessageId(format!("dingtalk-{}", uuid::Uuid::new_v4())))
    }
}

#[async_trait]
impl ChatChannelBackend for DingtalkBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Dingtalk
    }

    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
        runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        *self.status.lock().await = ChannelConnectionStatus::Connecting;
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);
        let opened = tokio::select! {
            result = self.open_stream() => result,
            _ = shutdown_rx.changed() => {
                *self.status.lock().await = ChannelConnectionStatus::Disconnected;
                return Err(ChatChannelError::ConnectionFailed("DingTalk startup cancelled".into()));
            }
        };
        let first_stream = match opened {
            Ok(stream) => stream,
            Err(error) => {
                *self.shutdown_tx.lock().await = None;
                *self.status.lock().await = ChannelConnectionStatus::Error;
                return Err(error);
            }
        };
        if *shutdown_rx.borrow() {
            *self.status.lock().await = ChannelConnectionStatus::Disconnected;
            return Err(ChatChannelError::ConnectionFailed(
                "DingTalk startup cancelled".into(),
            ));
        }
        *self.status.lock().await = ChannelConnectionStatus::Connected;
        let task = tokio::spawn(runtime::run_loop(
            self.clone(),
            first_stream,
            command_tx,
            runtime_tx,
            generation,
            shutdown_rx,
        ));
        *self.task.lock().await = Some(task);
        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        if let Some(sender) = self.shutdown_tx.lock().await.take() {
            let _ = sender.send(true);
        }
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
        *self.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.status.lock().await
    }

    async fn send_message(&self, _text: &str) -> Result<SentMessageId, ChatChannelError> {
        Err(ChatChannelError::ConfigurationInvalid(
            "DingTalk requires an inbound session target".into(),
        ))
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
        self.send_text_to(&message.to_plain_text(), target).await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        let mut stream = self.open_stream().await?;
        stream
            .close(None)
            .await
            .map_err(|error| ChatChannelError::ConnectionFailed(redact_transport_error(&error)))
    }
}

fn provider_code(value: &serde_json::Value) -> String {
    value
        .as_i64()
        .map(|code| code.to_string())
        .or_else(|| value.as_str().map(str::to_string))
        .unwrap_or_else(|| value.to_string())
}

fn provider_code_ok(value: &serde_json::Value) -> bool {
    value
        .as_i64()
        .map(|code| code == 0)
        .or_else(|| {
            value
                .as_str()
                .map(|code| code == "0" || code.eq_ignore_ascii_case("ok"))
        })
        .unwrap_or(false)
}

pub(super) fn redact_transport_error(error: &impl std::fmt::Display) -> String {
    let message = error.to_string();
    match message.find('?') {
        Some(index) => format!("{}?[redacted]", &message[..index]),
        None => message,
    }
}
