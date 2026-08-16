use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{tungstenite, MaybeTlsStream, WebSocketStream};

use super::DingtalkBackend;
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::{ChannelConnectionStatus, ChannelRuntimeEvent, IncomingCommand};

const MAX_RECONNECT_DELAY_SECS: u64 = 30;
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(15);
const STABLE_SESSION_DURATION: Duration = Duration::from_secs(30);

pub(super) type DingTalkSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(super) async fn await_registered(
    stream: &mut DingTalkSocket,
    channel_id: i32,
) -> Result<(), ChatChannelError> {
    tokio::time::timeout(REGISTRATION_TIMEOUT, async {
        loop {
            match stream.next().await {
                Some(Ok(tungstenite::Message::Text(text))) => {
                    if super::protocol::handle_registration_frame(text.as_ref(), stream, channel_id)
                        .await?
                    {
                        return Ok(());
                    }
                }
                Some(Ok(tungstenite::Message::Ping(data))) => {
                    stream
                        .send(tungstenite::Message::Pong(data))
                        .await
                        .map_err(|error| {
                            ChatChannelError::ConnectionFailed(super::redact_transport_error(
                                &error,
                            ))
                        })?;
                }
                Some(Ok(tungstenite::Message::Close(_))) | None => {
                    return Err(ChatChannelError::ConnectionFailed(
                        "DingTalk stream closed before registration".into(),
                    ));
                }
                Some(Err(error)) => {
                    return Err(ChatChannelError::ConnectionFailed(
                        super::redact_transport_error(&error),
                    ));
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| ChatChannelError::ConnectionFailed("DingTalk registration timed out".into()))?
}

pub(super) async fn run_loop(
    backend: DingtalkBackend,
    first_stream: DingTalkSocket,
    command_tx: mpsc::Sender<IncomingCommand>,
    runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
    generation: u64,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let mut stream = Some(first_stream);
    let mut attempt = 0u32;
    loop {
        let current = match stream.take() {
            Some(stream) => stream,
            None => match tokio::select! {
                result = backend.open_stream() => result,
                _ = shutdown_rx.changed() => break,
            } {
                Ok(stream) => stream,
                Err(error) => {
                    attempt = attempt.saturating_add(1);
                    report_error(&backend, &runtime_tx, generation, &error).await;
                    if wait_backoff(attempt, &mut shutdown_rx).await {
                        break;
                    }
                    continue;
                }
            },
        };
        if *shutdown_rx.borrow() {
            break;
        }
        let session_started = Instant::now();
        report_connected(&backend, &runtime_tx, generation).await;
        if *shutdown_rx.borrow() {
            break;
        }
        let result = run_stream(&backend, current, &command_tx, &mut shutdown_rx).await;
        if *shutdown_rx.borrow() {
            break;
        }
        if let Err(error) = result {
            report_error(&backend, &runtime_tx, generation, &error).await;
            if session_started.elapsed() >= STABLE_SESSION_DURATION {
                attempt = 0;
            }
            attempt = attempt.saturating_add(1);
            if wait_backoff(attempt, &mut shutdown_rx).await {
                break;
            }
        }
    }
    *backend.status.lock().await = ChannelConnectionStatus::Disconnected;
}

async fn run_stream(
    backend: &DingtalkBackend,
    stream: DingTalkSocket,
    command_tx: &mpsc::Sender<IncomingCommand>,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), ChatChannelError> {
    let (mut write, mut read) = stream.split();
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                let _ = write.close().await;
                return Ok(());
            }
            message = read.next() => match message {
                Some(Ok(tungstenite::Message::Text(text))) => {
                    super::protocol::handle_frame(
                        backend.channel_id,
                        text.as_ref(),
                        &mut write,
                        command_tx,
                        &backend.client,
                    )
                    .await?;
                }
                Some(Ok(tungstenite::Message::Ping(data))) => {
                    let _ = write.send(tungstenite::Message::Pong(data)).await;
                }
                Some(Ok(tungstenite::Message::Close(_))) | None => {
                    return Err(ChatChannelError::ConnectionFailed("DingTalk stream closed".into()));
                }
                Some(Err(error)) => {
                    return Err(ChatChannelError::ConnectionFailed(
                        super::redact_transport_error(&error),
                    ));
                }
                _ => {}
            }
        }
    }
}

async fn report_connected(
    backend: &DingtalkBackend,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    generation: u64,
) {
    if !set_status(&backend.status, ChannelConnectionStatus::Connected).await {
        return;
    }
    tracing::info!(
        channel_id = backend.channel_id,
        channel_type = "dingtalk",
        generation,
        stage = "stream_reconnect",
        "[DingTalk] stream connection recovered"
    );
    if let Err(error) = runtime_tx
        .send(ChannelRuntimeEvent::Connected {
            channel_id: backend.channel_id,
            generation,
        })
        .await
    {
        tracing::warn!(channel_id = backend.channel_id, generation, error = %error, "[DingTalk] runtime connected event delivery failed");
    }
}

async fn report_error(
    backend: &DingtalkBackend,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    generation: u64,
    error: &ChatChannelError,
) {
    if !set_status(&backend.status, ChannelConnectionStatus::Error).await {
        return;
    }
    tracing::warn!(
        channel_id = backend.channel_id,
        channel_type = "dingtalk",
        generation,
        stage = "stream_session",
        error_category = error.category(),
        error = %error,
        "[DingTalk] stream unavailable; reconnect scheduled"
    );
    if let Err(send_error) = runtime_tx
        .send(ChannelRuntimeEvent::Error {
            channel_id: backend.channel_id,
            generation,
            error: error.to_string(),
        })
        .await
    {
        tracing::warn!(channel_id = backend.channel_id, generation, error = %send_error, "[DingTalk] runtime error event delivery failed");
    }
}

async fn set_status(
    status: &Arc<Mutex<ChannelConnectionStatus>>,
    next: ChannelConnectionStatus,
) -> bool {
    let mut current = status.lock().await;
    if *current == next {
        return false;
    }
    *current = next;
    true
}

async fn wait_backoff(attempt: u32, shutdown: &mut tokio::sync::watch::Receiver<bool>) -> bool {
    let delay = (2u64.saturating_pow(attempt.min(5))).min(MAX_RECONNECT_DELAY_SECS);
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(delay)) => false,
        _ = shutdown.changed() => true,
    }
}
