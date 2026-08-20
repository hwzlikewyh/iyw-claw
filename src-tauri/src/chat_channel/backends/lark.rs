use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use prost::Message as ProstMessage;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite;

use crate::chat_channel::attachments::{AttachmentCapability, ChannelAttachment};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;

const TOKEN_REFRESH_MARGIN_SECS: u64 = 300;
const LARK_MAX_FILE_BYTES: u64 = 30 * 1024 * 1024;

// ── Lark WebSocket protobuf Frame (pbbp2) ──
// Source: larksuite/oapi-sdk-go ws/pbbp2.pb.go

const FRAME_METHOD_CONTROL: i32 = 0; // Ping/Pong
const FRAME_METHOD_DATA: i32 = 1; // Event/Card

#[derive(Clone, PartialEq, ProstMessage)]
struct Frame {
    #[prost(uint64, tag = 1)]
    seq_id: u64,
    #[prost(uint64, tag = 2)]
    log_id: u64,
    #[prost(int32, tag = 3)]
    service: i32,
    #[prost(int32, tag = 4)]
    method: i32,
    #[prost(message, repeated, tag = 5)]
    headers: Vec<FrameHeader>,
    #[prost(string, tag = 6)]
    payload_encoding: String,
    #[prost(string, tag = 7)]
    payload_type: String,
    #[prost(bytes = "vec", tag = 8)]
    payload: Vec<u8>,
    #[prost(string, tag = 9)]
    log_id_new: String,
}

#[derive(Clone, PartialEq, ProstMessage)]
struct FrameHeader {
    #[prost(string, tag = 1)]
    key: String,
    #[prost(string, tag = 2)]
    value: String,
}

impl Frame {
    fn get_header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.value.as_str())
    }

    fn set_header(&mut self, key: &str, value: &str) {
        if let Some(h) = self.headers.iter_mut().find(|h| h.key == key) {
            h.value = value.to_string();
        } else {
            self.headers.push(FrameHeader {
                key: key.to_string(),
                value: value.to_string(),
            });
        }
    }
}

// ── Lark REST API types ──

#[derive(Deserialize)]
struct TenantAccessTokenResponse {
    code: i32,
    msg: String,
    tenant_access_token: Option<String>,
    expire: Option<u64>,
}

#[derive(Serialize)]
struct SendMessageRequest {
    receive_id: String,
    msg_type: String,
    content: String,
}

#[derive(Deserialize)]
struct SendMessageResponse {
    code: i32,
    msg: String,
    data: Option<SendMessageData>,
}

#[derive(Deserialize)]
struct SendMessageData {
    message_id: Option<String>,
}

#[derive(Deserialize)]
struct UploadFileResponse {
    code: i32,
    data: Option<UploadFileData>,
}

#[derive(Deserialize)]
struct UploadFileData {
    file_key: Option<String>,
}

#[derive(Deserialize)]
struct WsConnectResponse {
    code: i32,
    msg: String,
    data: Option<WsConnectData>,
}

#[derive(Deserialize)]
struct WsConnectData {
    #[serde(rename = "URL")]
    url: Option<String>,
}

// ── Token cache ──

struct TokenCache {
    token: String,
    expires_at: Instant,
}

// ── Multi-part frame cache ──

struct PartialMessage {
    parts: HashMap<i32, Vec<u8>>,
    total: i32,
    created_at: Instant,
}

/// TTL for partial message reassembly entries. Prevents unbounded memory growth
/// if a multi-part message never completes (network issue, Lark SDK bug, etc).
const PARTIAL_MSG_TTL_SECS: u64 = 60;

// ── LarkBackend ──

#[derive(Clone)]
pub struct LarkBackend {
    app_id: String,
    app_secret: String,
    chat_id: String,
    api_base_url: &'static str,
    channel_id: i32,
    client: reqwest::Client,
    token_cache: Arc<RwLock<Option<TokenCache>>>,
    status: Arc<Mutex<ChannelConnectionStatus>>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
}

impl LarkBackend {
    pub fn new(
        channel_id: i32,
        app_id: String,
        app_secret: String,
        chat_id: String,
        region: LarkRegion,
    ) -> Self {
        Self {
            app_id,
            app_secret,
            chat_id,
            api_base_url: region.api_base_url(),
            channel_id,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            token_cache: Arc::new(RwLock::new(None)),
            status: Arc::new(Mutex::new(ChannelConnectionStatus::Disconnected)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_tenant_access_token(&self) -> Result<String, ChatChannelError> {
        {
            let cache = self.token_cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > Instant::now() {
                    return Ok(cached.token.clone());
                }
            }
        }

        let resp = self
            .client
            .post(format!(
                "{}/open-apis/auth/v3/tenant_access_token/internal",
                self.api_base_url
            ))
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret,
            }))
            .send()
            .await
            .map_err(|e| ChatChannelError::AuthenticationFailed(e.to_string()))?;

        let result: TenantAccessTokenResponse = resp
            .json()
            .await
            .map_err(|e| ChatChannelError::AuthenticationFailed(e.to_string()))?;

        if result.code != 0 {
            return Err(ChatChannelError::AuthenticationFailed(format!(
                "code={}, msg={}",
                result.code, result.msg
            )));
        }

        let token = result
            .tenant_access_token
            .ok_or_else(|| ChatChannelError::AuthenticationFailed("No token in response".into()))?;
        let expire_secs = result.expire.unwrap_or(7200);

        let expires_at = Instant::now()
            + Duration::from_secs(expire_secs.saturating_sub(TOKEN_REFRESH_MARGIN_SECS));
        *self.token_cache.write().await = Some(TokenCache {
            token: token.clone(),
            expires_at,
        });

        Ok(token)
    }

    async fn send_lark_message(
        &self,
        msg_type: &str,
        content: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_lark_message_to(&self.chat_id, msg_type, content)
            .await
    }

    async fn send_lark_message_to(
        &self,
        chat_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        if chat_id.trim().is_empty() {
            return Err(ChatChannelError::ConfigurationInvalid(
                "Lark target chat is empty".to_string(),
            ));
        }
        let token = self.get_tenant_access_token().await?;

        let resp = self
            .client
            .post(format!(
                "{}/open-apis/im/v1/messages?receive_id_type=chat_id",
                self.api_base_url
            ))
            .header("Authorization", format!("Bearer {}", token))
            .json(&SendMessageRequest {
                receive_id: chat_id.to_string(),
                msg_type: msg_type.to_string(),
                content: content.to_string(),
            })
            .send()
            .await
            .map_err(|e| ChatChannelError::SendFailed(e.to_string()))?;

        let result: SendMessageResponse = resp
            .json()
            .await
            .map_err(|e| ChatChannelError::SendFailed(e.to_string()))?;

        if result.code != 0 {
            return Err(ChatChannelError::SendFailed(format!(
                "code={}, msg={}",
                result.code, result.msg
            )));
        }

        let message_id = result.data.and_then(|d| d.message_id).unwrap_or_default();
        Ok(SentMessageId(message_id))
    }

    async fn upload_file(&self, file: &ChannelAttachment) -> Result<String, ChatChannelError> {
        let token = self.get_tenant_access_token().await?;
        let part = reqwest::multipart::Part::bytes(file.bytes.to_vec())
            .file_name(file.name.clone())
            .mime_str(&file.mime_type)
            .map_err(|_| ChatChannelError::SendFailed("invalid attachment MIME type".into()))?;
        let form = reqwest::multipart::Form::new()
            .text("file_type", "stream")
            .text("file_name", file.name.clone())
            .part("file", part);
        let response = self
            .client
            .post(format!("{}/open-apis/im/v1/files", self.api_base_url))
            .header("Authorization", format!("Bearer {token}"))
            .multipart(form)
            .send()
            .await
            .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?;
        let result: UploadFileResponse = response
            .json()
            .await
            .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?;
        if result.code != 0 {
            return Err(ChatChannelError::SendFailed(format!(
                "provider code {}",
                result.code
            )));
        }
        result
            .data
            .and_then(|data| data.file_key)
            .ok_or_else(|| ChatChannelError::SendFailed("provider file key missing".into()))
    }

    async fn start_ws_receiver(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
        runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        // A channel is not connected until the provider accepts the WebSocket
        // handshake. Do that work before returning from `start` so callers do
        // not mistake a spawned reconnect task for a live transport.
        let ws_url = fetch_ws_url(
            &self.client,
            self.api_base_url,
            &self.app_id,
            &self.app_secret,
        )
        .await?;
        let (initial_stream, _) =
            tokio_tungstenite::connect_async(&ws_url)
                .await
                .map_err(|error| {
                    ChatChannelError::ConnectionFailed(format!(
                        "Lark WebSocket handshake failed: {}",
                        redact_transport_error(&error)
                    ))
                })?;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let channel_id = self.channel_id;
        let sender = self.clone();
        let status = self.status.clone();
        let app_id = self.app_id.clone();
        let app_secret = self.app_secret.clone();
        let api_base_url = self.api_base_url;
        let client = self.client.clone();
        *status.lock().await = ChannelConnectionStatus::Connected;
        tracing::info!(
            channel_id,
            channel_type = "lark",
            generation,
            stage = "websocket_handshake",
            "[Lark] WebSocket handshake completed"
        );

        tokio::spawn(async move {
            let mut retry_count = 0u32;
            let mut next_stream = Some(initial_stream);

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let ws_stream = match next_stream.take() {
                    Some(stream) => stream,
                    None => {
                        let ws_url_result = tokio::select! {
                            result = fetch_ws_url(&client, api_base_url, &app_id, &app_secret) => Some(result),
                            _ = shutdown_rx.changed() => None,
                        };
                        let ws_url = match ws_url_result {
                            None => break,
                            Some(Ok(url)) => url,
                            Some(Err(error)) => {
                                report_transport_error(
                                    &status,
                                    &runtime_tx,
                                    channel_id,
                                    generation,
                                    "endpoint_fetch_failed",
                                    &redact_transport_error(&error),
                                )
                                .await;
                                if wait_for_retry(&mut shutdown_rx, &mut retry_count).await {
                                    break;
                                }
                                continue;
                            }
                        };
                        let connect_result = tokio::select! {
                            result = tokio_tungstenite::connect_async(&ws_url) => Some(result),
                            _ = shutdown_rx.changed() => None,
                        };
                        match connect_result {
                            None => break,
                            Some(Ok((stream, _))) => {
                                if *shutdown_rx.borrow() {
                                    break;
                                }
                                retry_count = 0;
                                let recovered = set_transport_status(
                                    &status,
                                    ChannelConnectionStatus::Connected,
                                )
                                .await;
                                if recovered {
                                    report_transport_connected(&runtime_tx, channel_id, generation)
                                        .await;
                                }
                                tracing::info!(
                                    channel_id,
                                    channel_type = "lark",
                                    generation,
                                    stage = "websocket_reconnect",
                                    "[Lark] WebSocket reconnected"
                                );
                                stream
                            }
                            Some(Err(error)) => {
                                report_transport_error(
                                    &status,
                                    &runtime_tx,
                                    channel_id,
                                    generation,
                                    "handshake_failed",
                                    &redact_transport_error(&error),
                                )
                                .await;
                                if wait_for_retry(&mut shutdown_rx, &mut retry_count).await {
                                    break;
                                }
                                continue;
                            }
                        }
                    }
                };

                let (mut write, mut read) = ws_stream.split();
                let mut partial_msgs: HashMap<String, PartialMessage> = HashMap::new();
                let mut last_partial_cleanup = Instant::now();

                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(tungstenite::Message::Binary(data))) => {
                                    match Frame::decode(data.as_ref()) {
                                        Ok(frame) => {
                                            let frame_type = frame.get_header("type").unwrap_or("").to_string();

                                            if frame.method == FRAME_METHOD_CONTROL {
                                                // Control frame: ping → respond with pong
                                                if frame_type == "ping" {
                                                    let mut pong = frame.clone();
                                                    // Clear type header and set to pong
                                                    pong.set_header("type", "pong");
                                                    pong.payload = Vec::new();
                                                    let mut buf = Vec::new();
                                                    if pong.encode(&mut buf).is_ok() {
                                                        let _ = write.send(tungstenite::Message::Binary(buf.into())).await;
                                                    }
                                                }
                                            } else if frame.method == FRAME_METHOD_DATA && frame_type == "event" {
                                                let start = Instant::now();

                                                // Multi-part reassembly
                                                let msg_id = frame.get_header("message_id").unwrap_or("").to_string();
                                                let sum: i32 = frame.get_header("sum").and_then(|s| s.parse().ok()).unwrap_or(1);
                                                let seq: i32 = frame.get_header("seq").and_then(|s| s.parse().ok()).unwrap_or(0);

                                                // Evict stale partial messages to prevent unbounded memory growth
                                if last_partial_cleanup.elapsed() > Duration::from_secs(PARTIAL_MSG_TTL_SECS) {
                                    partial_msgs.retain(|_, pm| pm.created_at.elapsed() < Duration::from_secs(PARTIAL_MSG_TTL_SECS));
                                    last_partial_cleanup = Instant::now();
                                }

                                let full_payload = if sum <= 1 {
                                                    Some(frame.payload.clone())
                                                } else {
                                                    let entry = partial_msgs.entry(msg_id.clone()).or_insert_with(|| PartialMessage {
                                                        parts: HashMap::new(),
                                                        total: sum,
                                                        created_at: Instant::now(),
                                                    });
                                                    entry.parts.insert(seq, frame.payload.clone());
                                                    if entry.parts.len() as i32 >= entry.total {
                                                        // All parts received — reassemble in order
                                                        let mut combined = Vec::new();
                                                        for i in 0..entry.total {
                                                            if let Some(part) = entry.parts.get(&i) {
                                                                combined.extend_from_slice(part);
                                                            }
                                                        }
                                                        partial_msgs.remove(&msg_id);
                                                        Some(combined)
                                                    } else {
                                                        None // Still waiting for more parts
                                                    }
                                                };

                                                if let Some(payload_bytes) = full_payload {
                                                    // Process event
                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes) {
                                                        if let Ok(event) = serde_json::from_str::<serde_json::Value>(payload_str) {
                                                            handle_lark_event(&event, channel_id, &command_tx, &sender).await;
                                                        } else {
                                                            tracing::info!("[Lark] event payload is not valid JSON");
                                                        }
                                                    }

                                                    // Send acknowledgment: echo frame back with {"code":200}
                                                    let elapsed_ms = start.elapsed().as_millis();
                                                    let mut ack = frame.clone();
                                                    ack.payload = br#"{"code":200}"#.to_vec();
                                                    ack.set_header("biz_rt", &elapsed_ms.to_string());
                                                    let mut buf = Vec::new();
                                                    if ack.encode(&mut buf).is_ok() {
                                                        let _ = write.send(tungstenite::Message::Binary(buf.into())).await;
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("[Lark] protobuf decode error: {e}, len={}", data.len());
                                        }
                                    }
                                }
                                Some(Ok(tungstenite::Message::Ping(data))) => {
                                    let _ = write.send(tungstenite::Message::Pong(data)).await;
                                }
                                Some(Ok(tungstenite::Message::Close(_))) | None => {
                                    if !*shutdown_rx.borrow() {
                                        report_transport_error(
                                            &status,
                                            &runtime_tx,
                                            channel_id,
                                            generation,
                                            "closed",
                                            "provider closed the WebSocket",
                                        ).await;
                                    }
                                    break;
                                }
                                Some(Err(error)) => {
                                    if !*shutdown_rx.borrow() {
                                        report_transport_error(
                                            &status,
                                            &runtime_tx,
                                            channel_id,
                                            generation,
                                            "read_failed",
                                            &redact_transport_error(&error),
                                        ).await;
                                    }
                                    break;
                                }
                                _ => {}
                            }
                        }
                        _ = shutdown_rx.changed() => {
                            let _ = write.close().await;
                            *status.lock().await = ChannelConnectionStatus::Disconnected;
                            return;
                        }
                    }
                }

                if wait_for_retry(&mut shutdown_rx, &mut retry_count).await {
                    break;
                }
            }

            *status.lock().await = ChannelConnectionStatus::Disconnected;
        });

        Ok(())
    }
}

async fn wait_for_retry(
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
    retry_count: &mut u32,
) -> bool {
    let delay = Duration::from_secs((2u64).pow((*retry_count).min(5)));
    *retry_count = (*retry_count).saturating_add(1);
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        _ = shutdown_rx.changed() => true,
    }
}

async fn set_transport_status(
    status: &Mutex<ChannelConnectionStatus>,
    next: ChannelConnectionStatus,
) -> bool {
    let mut current = status.lock().await;
    if *current == next {
        return false;
    }
    *current = next;
    true
}

async fn report_transport_connected(
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    channel_id: i32,
    generation: u64,
) {
    if let Err(error) = runtime_tx
        .send(ChannelRuntimeEvent::Connected {
            channel_id,
            generation,
        })
        .await
    {
        tracing::warn!(channel_id, generation, error = %error, "[Lark] runtime connected event delivery failed");
    }
}

async fn report_transport_error(
    status: &Mutex<ChannelConnectionStatus>,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    channel_id: i32,
    generation: u64,
    reason: &'static str,
    detail: &str,
) {
    if !set_transport_status(status, ChannelConnectionStatus::Error).await {
        return;
    }
    let error = format!("Lark WebSocket transport {reason}: {detail}");
    tracing::warn!(
        channel_id,
        channel_type = "lark",
        generation,
        stage = "websocket_session",
        error_category = reason,
        error = %error,
        "[Lark] transport unavailable; reconnect scheduled"
    );
    if let Err(send_error) = runtime_tx
        .send(ChannelRuntimeEvent::Error {
            channel_id,
            generation,
            error,
        })
        .await
    {
        tracing::warn!(channel_id, generation, error = %send_error, "[Lark] runtime error event delivery failed");
    }
}

fn redact_transport_error(error: &impl std::fmt::Display) -> String {
    let message = error.to_string();
    match message.find('?') {
        Some(index) => format!("{}?[redacted]", &message[..index]),
        None => message,
    }
}

async fn handle_lark_event(
    event: &serde_json::Value,
    channel_id: i32,
    command_tx: &mpsc::Sender<IncomingCommand>,
    sender: &LarkBackend,
) {
    let event_type = event
        .pointer("/header/event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if event_type == "im.message.receive_v1" {
        let msg_type = event
            .pointer("/event/message/message_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if msg_type != "text" {
            return;
        }

        // Group chat filtering: only process if bot is mentioned
        let chat_type = event
            .pointer("/event/message/chat_type")
            .and_then(|v| v.as_str())
            .unwrap_or("p2p");

        if chat_type == "group" {
            let mentions = event
                .pointer("/event/message/mentions")
                .and_then(|v| v.as_array());
            if mentions.is_none() || mentions.unwrap().is_empty() {
                return; // No mentions in group chat, ignore
            }
        }

        let content_str = event
            .pointer("/event/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Content is JSON string: {"text":"actual message"}
        let text = serde_json::from_str::<serde_json::Value>(content_str)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default();

        if text.is_empty() {
            return;
        }

        // Strip mention placeholders (e.g. "@_user_1") from text
        let clean_text = strip_lark_mentions(&text, event);

        if clean_text.is_empty() {
            return;
        }

        let sender_id = event
            .pointer("/event/sender/sender_id/open_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        tracing::info!(
            channel_id,
            content_chars = clean_text.chars().count(),
            "[Lark] incoming message accepted"
        );

        let provider_message_id = event
            .pointer("/event/message/message_id")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
            .unwrap_or_else(|| format!("l{}", lark_message_hash(&sender_id, &clean_text)));
        let chat_id = event
            .pointer("/event/message/chat_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let command = IncomingCommand {
            channel_id,
            sender_id,
            sender_name: None,
            command_text: clean_text,
            callback_data: None,
            target: ChannelMessageTarget {
                channel_id,
                chat_id: Some(chat_id.clone()),
                thread_key: None,
                thread_kind: Some("lark_chat".to_string()),
                provider_payload: None,
            },
            metadata: serde_json::json!({}),
            message_trace_id: super::super::dedupe::new_message_trace_id(channel_id),
            provider_message_id: Some(provider_message_id),
            received_at: chrono::Utc::now(),
        };
        // Bounded queue: never silently drop; reply busy so the sender retries.
        if let Err(error) = command_tx.try_send(command) {
            match error {
                mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!("[Lark] dispatcher queue full; replying busy");
                    let _ = sender
                        .send_lark_message_to(&chat_id, "text", super::DISPATCHER_BUSY_TEXT)
                        .await;
                }
                mpsc::error::TrySendError::Closed(_) => {
                    tracing::error!("[Lark] command channel closed; dropping inbound");
                }
            }
        }
    }
}

/// Strip Lark mention placeholders (e.g. `@_user_1`) from the message text.
fn strip_lark_mentions(text: &str, event: &serde_json::Value) -> String {
    let mut result = text.to_string();
    if let Some(mentions) = event
        .pointer("/event/message/mentions")
        .and_then(|v| v.as_array())
    {
        for mention in mentions {
            if let Some(key) = mention.get("key").and_then(|v| v.as_str()) {
                result = result.replace(key, "");
            }
        }
    }
    result.trim().to_string()
}

/// Fetch a fresh WebSocket endpoint URL from Feishu.
async fn fetch_ws_url(
    client: &reqwest::Client,
    api_base_url: &str,
    app_id: &str,
    app_secret: &str,
) -> Result<String, ChatChannelError> {
    let resp = client
        .post(format!("{api_base_url}/callback/ws/endpoint"))
        .json(&serde_json::json!({
            "AppID": app_id,
            "AppSecret": app_secret,
        }))
        .send()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(e.to_string()))?;

    let ws_resp: WsConnectResponse = resp
        .json()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(e.to_string()))?;

    if ws_resp.code != 0 {
        return Err(ChatChannelError::ConnectionFailed(format!(
            "WS connect failed: code={}, msg={}",
            ws_resp.code, ws_resp.msg
        )));
    }

    ws_resp
        .data
        .and_then(|d| d.url)
        .ok_or_else(|| ChatChannelError::ConnectionFailed("No WebSocket URL returned".into()))
}

#[async_trait]
impl ChatChannelBackend for LarkBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Lark
    }

    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
        runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        *self.status.lock().await = ChannelConnectionStatus::Connecting;
        if let Err(error) = self.get_tenant_access_token().await {
            *self.status.lock().await = ChannelConnectionStatus::Error;
            tracing::warn!(
                channel_id = self.channel_id,
                channel_type = "lark",
                generation,
                stage = "credential_validation",
                error_category = error.category(),
                error = %error,
                "[Lark] token validation failed during startup"
            );
            return Err(error);
        }
        if let Err(error) = self
            .start_ws_receiver(command_tx, runtime_tx, generation)
            .await
        {
            *self.status.lock().await = ChannelConnectionStatus::Error;
            tracing::warn!(
                channel_id = self.channel_id,
                channel_type = "lark",
                generation,
                stage = "websocket_startup",
                error_category = error.category(),
                error = %error,
                "[Lark] WebSocket startup failed"
            );
            return Err(error);
        }

        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(true);
        }
        *self.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.status.lock().await
    }

    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        let content = serde_json::json!({ "text": text }).to_string();
        self.send_lark_message("text", &content).await
    }

    async fn send_rich_message(
        &self,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        let card = build_lark_card(message);
        let content = serde_json::to_string(&card)
            .map_err(|e| ChatChannelError::SendFailed(e.to_string()))?;
        self.send_lark_message("interactive", &content).await
    }

    async fn send_rich_message_to(
        &self,
        message: &RichMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let chat_id = target.chat_id.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("Lark target chat is missing".to_string())
        })?;
        let card = build_lark_card(message);
        let content = serde_json::to_string(&card)
            .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?;
        self.send_lark_message_to(chat_id, "interactive", &content)
            .await
    }

    fn attachment_capability(&self) -> AttachmentCapability {
        AttachmentCapability {
            supported: true,
            max_file_bytes: Some(LARK_MAX_FILE_BYTES),
        }
    }

    async fn send_attachment_to(
        &self,
        attachment: &ChannelAttachment,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let chat_id = target.chat_id.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("Lark target chat is missing".to_string())
        })?;
        let file_key = self.upload_file(attachment).await?;
        let content = serde_json::json!({ "file_key": file_key }).to_string();
        self.send_lark_message_to(chat_id, "file", &content).await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        self.get_tenant_access_token().await?;
        let ws_url = fetch_ws_url(
            &self.client,
            self.api_base_url,
            &self.app_id,
            &self.app_secret,
        )
        .await?;
        let (mut stream, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|error| {
                ChatChannelError::ConnectionFailed(format!(
                    "Lark WebSocket handshake failed: {}",
                    redact_transport_error(&error)
                ))
            })?;
        stream.close(None).await.map_err(|error| {
            ChatChannelError::ConnectionFailed(format!(
                "Lark WebSocket close failed: {}",
                redact_transport_error(&error)
            ))
        })
    }
}

fn build_lark_card(msg: &RichMessage) -> serde_json::Value {
    let header_color = match msg.level {
        MessageLevel::Info => "blue",
        MessageLevel::Warning => "orange",
        MessageLevel::Error => "red",
    };

    let title = msg.title.as_deref().unwrap_or("iyw-claw");

    let mut elements: Vec<serde_json::Value> = Vec::new();

    if !msg.body.is_empty() {
        // Render the body as PLAIN TEXT, never markdown. An event body can carry
        // user-authored content (e.g. `user_prompt_sent` forwards the prompt
        // text verbatim), and Lark's `markdown` element would interpret `<at>`
        // mentions, links, and formatting embedded in it. `plain_text`
        // neutralizes all of that; existing event bodies are plain sentences, so
        // their rendering is unchanged.
        elements.push(serde_json::json!({
            "tag": "div",
            "text": {
                "tag": "plain_text",
                "content": msg.body,
            },
        }));
    }

    if !msg.fields.is_empty() {
        // Render each field as PLAIN TEXT, never `lark_md`. A field VALUE can carry
        // untrusted content — `permission_request` puts the proposed tool operation
        // (e.g. a Bash command) here, and `error` puts the agent's error string —
        // which `lark_md` would interpret, letting embedded `<at>` mentions, links
        // and markdown inject into the card. The label is our own i18n constant, so
        // the only injectable part is the value; plain_text neutralizes both. We
        // drop the bold label styling (cosmetic) for safety, mirroring the body.
        let field_elements: Vec<serde_json::Value> = msg
            .fields
            .iter()
            .map(|(k, v)| {
                serde_json::json!({
                    "is_short": true,
                    "text": {
                        "tag": "plain_text",
                        "content": format!("{}\n{}", k, v),
                    }
                })
            })
            .collect();

        elements.push(serde_json::json!({
            "tag": "div",
            "fields": field_elements,
        }));
    }

    serde_json::json!({
        "config": { "wide_screen_mode": true },
        "header": {
            "title": {
                "tag": "plain_text",
                "content": title,
            },
            "template": header_color,
        },
        "elements": elements,
    })
}

/// Deterministic composite hash for Lark events without a message id
/// (idempotency key fallback).
fn lark_message_hash(sender_id: &str, text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    sender_id.hash(&mut hasher);
    text.hash(&mut hasher);
    hasher.finish()
}
