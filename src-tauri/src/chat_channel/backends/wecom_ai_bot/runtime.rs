use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, watch};

use super::{protocol, OutboundRequest, State};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::*;

pub(crate) use protocol::{
    proactive_frame, reply_frame, target_chat_type, target_payload, verify_connection,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const MAX_SILENT_INTERVALS: u32 = 3;
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
const OUTBOUND_ACK_TIMEOUT: Duration = Duration::from_secs(10);
const OUTBOUND_ACK_SWEEP: Duration = Duration::from_secs(1);
const MAX_PENDING_ACKS: usize = 128;

struct PendingAck {
    sent_at: Instant,
    result_tx: oneshot::Sender<Result<SentMessageId, ChatChannelError>>,
}

pub(crate) struct RunArgs {
    pub(crate) channel_id: i32,
    pub(crate) bot_id: String,
    pub(crate) secret: String,
    pub(crate) endpoint: String,
    pub(crate) state: Arc<State>,
    pub(crate) command_tx: mpsc::Sender<IncomingCommand>,
    pub(crate) runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
    pub(crate) generation: u64,
    pub(crate) stop_rx: watch::Receiver<bool>,
    pub(crate) outbound_rx: mpsc::Receiver<OutboundRequest>,
    pub(crate) ready_tx: Option<oneshot::Sender<Result<(), ChatChannelError>>>,
}

pub(crate) async fn run_loop(mut args: RunArgs) {
    let mut connected_once = false;
    let mut failures = 0;
    while !*args.stop_rx.borrow() {
        set_status(&args.state, ChannelConnectionStatus::Connecting).await;
        let Some(outcome) = connect_or_stop(&mut args).await else {
            break;
        };
        let error = match outcome {
            Ok(stream) => {
                let recovered = connected_once;
                connected_once = true;
                failures = 0;
                set_status(&args.state, ChannelConnectionStatus::Connected).await;
                resolve_ready(&mut args.ready_tx, Ok(()));
                if recovered {
                    emit_connected(&args).await;
                }
                match run_session(stream, &mut args).await {
                    Ok(()) => break,
                    Err(error) => error,
                }
            }
            Err(error) if !connected_once => {
                set_status(&args.state, ChannelConnectionStatus::Error).await;
                resolve_ready(&mut args.ready_tx, Err(error));
                break;
            }
            Err(error) => error,
        };
        report_disconnect(&args, &error).await;
        reject_pending(&mut args.outbound_rx);
        failures += 1;
        if is_terminal(&error) || failures >= MAX_RECONNECT_ATTEMPTS {
            break;
        }
        if wait_reconnect(&mut args.stop_rx, failures).await {
            break;
        }
    }
    finish_run(&mut args).await;
}

async fn connect_or_stop(
    args: &mut RunArgs,
) -> Option<Result<protocol::WsStream, ChatChannelError>> {
    let outcome = tokio::select! {
        result = protocol::connect_and_subscribe(
            &args.endpoint,
            &args.bot_id,
            &args.secret,
        ) => Some(result),
        _ = args.stop_rx.changed() => None,
    };
    (!*args.stop_rx.borrow()).then_some(outcome).flatten()
}

async fn finish_run(args: &mut RunArgs) {
    reject_pending(&mut args.outbound_rx);
    resolve_ready(
        &mut args.ready_tx,
        Err(ChatChannelError::ConnectionFailed("stopped".into())),
    );
    if *args.stop_rx.borrow() {
        set_status(&args.state, ChannelConnectionStatus::Disconnected).await;
    }
}

async fn run_session(
    stream: protocol::WsStream,
    args: &mut RunArgs,
) -> Result<(), ChatChannelError> {
    let (mut write, mut read) = stream.split();
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    let mut ack_sweep = tokio::time::interval(OUTBOUND_ACK_SWEEP);
    let mut pending_acks = HashMap::new();
    let mut last_activity = Instant::now();
    loop {
        tokio::select! {
            changed = args.stop_rx.changed() => {
                if changed.is_err() || *args.stop_rx.borrow() {
                    let _ = write.close().await;
                    return Ok(());
                }
            }
            _ = heartbeat.tick() => {
                if last_activity.elapsed() > HEARTBEAT_INTERVAL * MAX_SILENT_INTERVALS {
                    return Err(ChatChannelError::ConnectionFailed("heartbeat response timed out".into()));
                }
                protocol::write_json(&mut write, protocol::ping_frame()).await?;
            }
            _ = ack_sweep.tick() => expire_pending_acks(&mut pending_acks),
            request = args.outbound_rx.recv() => match request {
                Some(request) => {
                    queue_outbound(&mut write, &mut pending_acks, request).await?;
                }
                None => return Ok(()),
            },
            message = read.next() => match message {
                Some(Ok(message)) => {
                    last_activity = Instant::now();
                    if let Some(ack) = protocol::handle_message(message, &mut write, args).await? {
                        resolve_provider_ack(&mut pending_acks, ack);
                    }
                }
                Some(Err(error)) => return Err(ChatChannelError::ConnectionFailed(error.to_string())),
                None => return Err(ChatChannelError::ConnectionFailed("WebSocket closed".into())),
            },
        }
    }
}

async fn queue_outbound(
    write: &mut protocol::WsSink,
    pending: &mut HashMap<String, PendingAck>,
    request: OutboundRequest,
) -> Result<(), ChatChannelError> {
    let Some(req_id) = protocol::frame_request_id(&request.frame).map(str::to_string) else {
        let _ = request.result_tx.send(Err(ChatChannelError::SendFailed(
            "WeCom outbound frame omitted req_id".into(),
        )));
        return Ok(());
    };
    if pending.len() >= MAX_PENDING_ACKS || pending.contains_key(&req_id) {
        let _ = request.result_tx.send(Err(ChatChannelError::SendFailed(
            "WeCom outbound acknowledgement queue is full".into(),
        )));
        return Ok(());
    }
    if let Err(error) = protocol::write_json(write, request.frame).await {
        let _ = request
            .result_tx
            .send(Err(ChatChannelError::SendFailed(error.to_string())));
        return Err(ChatChannelError::ConnectionFailed(
            "WeCom outbound write failed".into(),
        ));
    }
    pending.insert(
        req_id,
        PendingAck {
            sent_at: Instant::now(),
            result_tx: request.result_tx,
        },
    );
    Ok(())
}

fn resolve_provider_ack(pending: &mut HashMap<String, PendingAck>, ack: protocol::ProviderAck) {
    let Some(pending) = pending.remove(&ack.req_id) else {
        return;
    };
    let result = match ack.error {
        Some(error) => Err(ChatChannelError::SendFailed(error)),
        None => Ok(SentMessageId(format!("wecom-ai-bot-{}", ack.req_id))),
    };
    let _ = pending.result_tx.send(result);
}

fn expire_pending_acks(pending: &mut HashMap<String, PendingAck>) {
    let expired: Vec<String> = pending
        .iter()
        .filter(|(_, item)| item.sent_at.elapsed() >= OUTBOUND_ACK_TIMEOUT)
        .map(|(req_id, _)| req_id.clone())
        .collect();
    for req_id in expired {
        if let Some(pending) = pending.remove(&req_id) {
            let _ = pending.result_tx.send(Err(ChatChannelError::SendFailed(
                "WeCom outbound acknowledgement timed out".into(),
            )));
        }
    }
}

async fn report_disconnect(args: &RunArgs, error: &ChatChannelError) {
    if set_status(&args.state, ChannelConnectionStatus::Error).await {
        tracing::warn!(
            channel_id = args.channel_id,
            channel_type = "wecom_ai_bot",
            generation = args.generation,
            stage = "websocket_session",
            error_category = error.category(),
            error = %error,
            "[WeComAiBot] connection lost"
        );
        if let Err(send_error) = args
            .runtime_tx
            .send(ChannelRuntimeEvent::Error {
                channel_id: args.channel_id,
                generation: args.generation,
                error: error.to_string(),
            })
            .await
        {
            tracing::warn!(channel_id = args.channel_id, generation = args.generation, error = %send_error, "[WeComAiBot] runtime error event delivery failed");
        }
    }
}

async fn emit_connected(args: &RunArgs) {
    tracing::info!(
        channel_id = args.channel_id,
        channel_type = "wecom_ai_bot",
        generation = args.generation,
        stage = "websocket_reconnect",
        "[WeComAiBot] connection recovered"
    );
    if let Err(error) = args
        .runtime_tx
        .send(ChannelRuntimeEvent::Connected {
            channel_id: args.channel_id,
            generation: args.generation,
        })
        .await
    {
        tracing::warn!(channel_id = args.channel_id, generation = args.generation, error = %error, "[WeComAiBot] runtime connected event delivery failed");
    }
}

async fn set_status(state: &State, next: ChannelConnectionStatus) -> bool {
    let mut status = state.status.lock().await;
    let changed = *status != next;
    *status = next;
    changed
}

async fn wait_reconnect(stop_rx: &mut watch::Receiver<bool>, failures: u32) -> bool {
    let delay = Duration::from_secs(1_u64 << failures.saturating_sub(1).min(5));
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = stop_rx.changed() => changed.is_err() || *stop_rx.borrow(),
    }
}

fn is_terminal(error: &ChatChannelError) -> bool {
    matches!(
        error,
        ChatChannelError::AuthenticationFailed(_)
            | ChatChannelError::ConfigurationInvalid(_)
            | ChatChannelError::AlreadyConnected
    )
}

fn resolve_ready(
    ready: &mut Option<oneshot::Sender<Result<(), ChatChannelError>>>,
    result: Result<(), ChatChannelError>,
) {
    if let Some(sender) = ready.take() {
        let _ = sender.send(result);
    }
}

fn reject_pending(outbound_rx: &mut mpsc::Receiver<OutboundRequest>) {
    while let Ok(request) = outbound_rx.try_recv() {
        let _ = request.result_tx.send(Err(ChatChannelError::NotConnected));
    }
}
