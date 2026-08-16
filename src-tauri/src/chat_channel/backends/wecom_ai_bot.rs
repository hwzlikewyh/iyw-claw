//! 企业微信智能机器人 WebSocket 长连接后端。

mod protocol;
mod runtime;

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio::task::JoinHandle;

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;

const DEFAULT_WS_URL: &str = "wss://openws.work.weixin.qq.com";
const START_TIMEOUT: Duration = Duration::from_secs(15);
const OUTBOUND_QUEUE_CAPACITY: usize = 128;

pub struct WecomAiBotBackend {
    channel_id: i32,
    config: WecomAiBotConfig,
    secret: String,
    state: Arc<State>,
}

pub(crate) struct State {
    pub(crate) status: Mutex<ChannelConnectionStatus>,
    pub(crate) stop_tx: Mutex<Option<watch::Sender<bool>>>,
    pub(crate) outbound_tx: Mutex<Option<mpsc::Sender<OutboundRequest>>>,
    pub(crate) task: Mutex<Option<JoinHandle<()>>>,
}

pub(crate) struct OutboundRequest {
    pub(crate) frame: Value,
    pub(crate) result_tx: oneshot::Sender<Result<SentMessageId, ChatChannelError>>,
}

impl WecomAiBotBackend {
    pub fn new(channel_id: i32, config: WecomAiBotConfig, secret: String) -> Self {
        Self {
            channel_id,
            config,
            secret,
            state: Arc::new(State {
                status: Mutex::new(ChannelConnectionStatus::Disconnected),
                stop_tx: Mutex::new(None),
                outbound_tx: Mutex::new(None),
                task: Mutex::new(None),
            }),
        }
    }

    fn validate_config(&self) -> Result<(), ChatChannelError> {
        if self.config.bot_id.trim().is_empty() || self.secret.trim().is_empty() {
            return Err(ChatChannelError::ConfigurationInvalid(
                "WeCom AI Bot ID and Secret are required".to_string(),
            ));
        }
        Ok(())
    }

    async fn send_frame(&self, frame: Value) -> Result<SentMessageId, ChatChannelError> {
        if self.status().await != ChannelConnectionStatus::Connected {
            return Err(ChatChannelError::NotConnected);
        }
        let tx = self
            .state
            .outbound_tx
            .lock()
            .await
            .clone()
            .ok_or(ChatChannelError::NotConnected)?;
        let (result_tx, result_rx) = oneshot::channel();
        tx.try_send(OutboundRequest { frame, result_tx })
            .map_err(|_| ChatChannelError::SendFailed("WeCom outbound queue is full".into()))?;
        tokio::time::timeout(START_TIMEOUT, result_rx)
            .await
            .map_err(|_| ChatChannelError::SendFailed("WeCom send timed out".into()))?
            .map_err(|_| ChatChannelError::NotConnected)?
    }

    async fn start_worker(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
        runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        self.stop().await?;
        let (stop_tx, stop_rx) = watch::channel(false);
        let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        let (ready_tx, ready_rx) = oneshot::channel();
        *self.state.stop_tx.lock().await = Some(stop_tx);
        *self.state.outbound_tx.lock().await = Some(outbound_tx);
        let task = tokio::spawn(runtime::run_loop(runtime::RunArgs {
            channel_id: self.channel_id,
            bot_id: self.config.bot_id.clone(),
            secret: self.secret.clone(),
            endpoint: DEFAULT_WS_URL.to_string(),
            state: Arc::clone(&self.state),
            command_tx,
            runtime_tx,
            generation,
            stop_rx,
            outbound_rx,
            ready_tx: Some(ready_tx),
        }));
        *self.state.task.lock().await = Some(task);
        let result = match tokio::time::timeout(START_TIMEOUT, ready_rx).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(ChatChannelError::ConnectionFailed(
                "connection task stopped".into(),
            )),
            Err(_) => Err(ChatChannelError::ConnectionFailed(
                "subscribe handshake timed out".into(),
            )),
        };
        if result.is_err() {
            self.stop().await?;
        }
        result
    }

    fn proactive_frame(&self, chatid: &str, text: &str) -> Result<Value, ChatChannelError> {
        if chatid.trim().is_empty() {
            return Err(ChatChannelError::ConfigurationInvalid(
                "WeCom AI Bot target chat is empty".to_string(),
            ));
        }
        Ok(runtime::proactive_frame(chatid, text))
    }
}

#[async_trait::async_trait]
impl ChatChannelBackend for WecomAiBotBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::WecomAiBot
    }

    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
        runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        self.validate_config()?;
        *self.state.status.lock().await = ChannelConnectionStatus::Connecting;
        let result = self.start_worker(command_tx, runtime_tx, generation).await;
        if result.is_err() {
            *self.state.status.lock().await = ChannelConnectionStatus::Error;
        }
        result
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        if let Some(stop_tx) = self.state.stop_tx.lock().await.take() {
            let _ = stop_tx.send(true);
        }
        if let Some(task) = self.state.task.lock().await.take() {
            let _ = task.await;
        }
        *self.state.outbound_tx.lock().await = None;
        *self.state.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.state.status.lock().await
    }

    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        self.send_frame(self.proactive_frame(&self.config.default_chatid, text)?)
            .await
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
        let text = message.to_plain_text();
        let req_id = runtime::target_payload(target, "req_id");
        let chatid = target
            .chat_id
            .as_deref()
            .or_else(|| runtime::target_payload(target, "chatid"));
        let frame = match req_id {
            Some(req_id) if !req_id.is_empty() => runtime::reply_frame(req_id, &text),
            _ => self.proactive_frame(chatid.unwrap_or_default(), &text)?,
        };
        self.send_frame(frame).await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        self.validate_config()?;
        runtime::verify_connection(DEFAULT_WS_URL, &self.config.bot_id, &self.secret).await
    }
}
