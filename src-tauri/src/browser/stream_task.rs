use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::error::{BrowserError, BrowserErrorCode};
use super::frame_protocol::{encode_frame, frame_sequence};
use super::stream::StreamControl;
use super::tab_metadata::response_data;
use super::types::{BrowserFrameSubscriptionStatus, BrowserGenerations};

const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_MESSAGE_SIZE: usize = 13 * 1024 * 1024;
const MAX_CONSECUTIVE_FRAME_FAILURES: usize = 5;

pub(super) struct StreamTaskContext {
    pub session: String,
    pub cli: AgentBrowserCli,
    pub cdp_url: String,
    pub generations: BrowserGenerations,
    pub channel: Channel<InvokeResponseBody>,
    pub cancellation: CancellationToken,
    pub control: mpsc::Receiver<StreamControl>,
    pub status: Arc<RwLock<BrowserFrameSubscriptionStatus>>,
}

pub(super) async fn run(mut context: StreamTaskContext) -> Result<(), BrowserError> {
    let url = stream_url(&context).await?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_SIZE))
        .max_frame_size(Some(MAX_MESSAGE_SIZE));
    let connection = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(url, Some(config), false),
    );
    let (socket, _) = tokio::select! {
        _ = context.cancellation.cancelled() => return Ok(()),
        result = connection => result.map_err(|_| disconnected())?
            .map_err(|_| disconnected())?,
    };
    *context.status.write().await = BrowserFrameSubscriptionStatus::Streaming;
    let (mut sink, mut source) = socket.split();
    let mut pending_seq = None;
    let mut last_seq = None;
    let mut frame_failures = 0_usize;
    loop {
        tokio::select! {
            _ = context.cancellation.cancelled() => {
                let _ = sink.send(Message::Close(None)).await;
                return Ok(());
            }
            control = context.control.recv() => {
                let Some(control) = control else { return Ok(()); };
                handle_control(control, &mut sink, &mut pending_seq).await;
            }
            incoming = source.next() => {
                let Some(incoming) = incoming else { return Err(disconnected()); };
                let message = incoming.map_err(|_| disconnected())?;
                match message {
                    Message::Text(text) => {
                        match encode_frame(&text, &context.generations) {
                            Ok(Some((seq, bytes))) => {
                                frame_failures = 0;
                                if last_seq.is_some_and(|last| seq <= last) || pending_seq.is_some() {
                                    tracing::warn!(
                                        target: "iyw_claw_browser",
                                        session = %context.session,
                                        seq,
                                        last_seq = ?last_seq,
                                        pending_seq = ?pending_seq,
                                        "browser frame dropped because delivery is still pending"
                                    );
                                    send_consumed_ack(&mut sink, seq).await?;
                                    continue;
                                }
                                context.channel
                                    .send(InvokeResponseBody::Raw(bytes))
                                    .map_err(|_| disconnected())?;
                                pending_seq = Some(seq);
                                last_seq = Some(seq);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                frame_failures += 1;
                                let seq = frame_sequence(&text);
                                tracing::warn!(
                                    target: "iyw_claw_browser",
                                    session = %context.session,
                                    seq = ?seq,
                                    message_bytes = text.len(),
                                    consecutive_failures = frame_failures,
                                    "browser frame payload was dropped"
                                );
                                if let Some(seq) = seq {
                                    send_consumed_ack(&mut sink, seq).await?;
                                }
                                if frame_failures >= MAX_CONSECUTIVE_FRAME_FAILURES {
                                    return Err(error);
                                }
                            }
                        }
                    }
                    Message::Ping(data) => sink.send(Message::Pong(data)).await.map_err(|_| disconnected())?,
                    Message::Close(_) => return Err(disconnected()),
                    _ => {}
                }
            }
        }
    }
}

async fn handle_control<S>(control: StreamControl, sink: &mut S, pending_seq: &mut Option<u64>)
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    match control {
        StreamControl::Ack { seq, response } => {
            let result = send_ack(sink, pending_seq, seq).await;
            let _ = response.send(result);
        }
        StreamControl::Input { messages, response } => {
            let result = send_input(sink, messages).await;
            let _ = response.send(result);
        }
    }
}

async fn send_ack<S>(
    sink: &mut S,
    pending_seq: &mut Option<u64>,
    seq: u64,
) -> Result<(), BrowserError>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    if *pending_seq != Some(seq) {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserStaleGeneration,
            "The browser frame acknowledgement is stale",
        ));
    }
    send_consumed_ack(sink, seq).await?;
    *pending_seq = None;
    Ok(())
}

async fn send_consumed_ack<S>(sink: &mut S, seq: u64) -> Result<(), BrowserError>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    sink.send(Message::Text(
        json!({ "type": "ack", "seq": seq }).to_string().into(),
    ))
    .await
    .map_err(|_| disconnected())
}

async fn send_input<S>(sink: &mut S, messages: Vec<Value>) -> Result<(), BrowserError>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let mut sent = 0_usize;
    for message in messages {
        if sink
            .send(Message::Text(message.to_string().into()))
            .await
            .is_err()
        {
            return Err(disconnected().effect_may_have_occurred(sent > 0));
        }
        sent += 1;
    }
    Ok(())
}

async fn stream_url(context: &StreamTaskContext) -> Result<String, BrowserError> {
    let response = context
        .cli
        .run_pinned(
            &context.session,
            &context.cdp_url,
            &["stream", "status"],
            STATUS_TIMEOUT,
            context.cancellation.clone(),
        )
        .await?;
    let mut data = response_data(&response);
    let enabled = data.get("enabled").and_then(Value::as_bool) == Some(true);
    let enabled_response;
    if !enabled {
        enabled_response = context
            .cli
            .run_pinned(
                &context.session,
                &context.cdp_url,
                &["stream", "enable"],
                STATUS_TIMEOUT,
                context.cancellation.clone(),
            )
            .await?;
        data = response_data(&enabled_response);
    }
    let port = data
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .ok_or_else(disconnected)?;
    Ok(format!("ws://127.0.0.1:{port}/?pacing=ack&maxFps=10"))
}

fn disconnected() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserStreamDisconnected,
        "The browser frame stream is disconnected",
    )
    .retryable(true)
}
