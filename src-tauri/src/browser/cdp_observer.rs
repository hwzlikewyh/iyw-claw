use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use super::cdp_errors::{command_rejected, timeout, unavailable};
use super::cdp_maps::update_protocol_maps;
use super::error::BrowserError;
use super::manager::BrowserSessionManager;

mod write;
use write::send_with_timeout;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const OBSERVER_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const SOCKET_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
struct CdpRequest {
    method: String,
    params: Value,
    session_id: Option<String>,
    response: oneshot::Sender<Result<Value, BrowserError>>,
}
#[derive(Debug, Clone)]
pub(super) struct CdpObserverHandle {
    commands: mpsc::Sender<CdpRequest>,
    cancellation: CancellationToken,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl CdpObserverHandle {
    pub async fn start(
        url: &str,
        download_path: &Path,
        manager: BrowserSessionManager,
        generation: u64,
        lifecycle: CancellationToken,
    ) -> Result<Self, BrowserError> {
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_SIZE))
            .max_frame_size(Some(MAX_MESSAGE_SIZE));
        let connection = tokio::select! {
            _ = lifecycle.cancelled() => return Err(BrowserError::shutting_down()),
            result = tokio::time::timeout(
                CONNECT_TIMEOUT,
                tokio_tungstenite::connect_async_with_config(url, Some(config), false),
            ) => result.map_err(|_| unavailable())?.map_err(|_| unavailable())?,
        };
        let (commands, receiver) = mpsc::channel(32);
        let cancellation = CancellationToken::new();
        let task = tokio::spawn(run_observer(
            connection.0,
            receiver,
            cancellation.clone(),
            manager,
            generation,
        ));
        let handle = Self {
            commands,
            cancellation,
            task: Arc::new(Mutex::new(Some(task))),
        };
        let result = {
            let initialize = handle.initialize(download_path);
            tokio::pin!(initialize);
            tokio::select! {
                _ = lifecycle.cancelled() => Err(BrowserError::shutting_down()),
                result = &mut initialize => result,
            }
        };
        if let Err(error) = result {
            handle.stop().await;
            return Err(error);
        }
        Ok(handle)
    }

    pub async fn call(
        &self,
        method: &str,
        params: Value,
        session_id: Option<String>,
    ) -> Result<Value, BrowserError> {
        let (response, result) = oneshot::channel();
        self.commands
            .send(CdpRequest {
                method: method.to_string(),
                params,
                session_id,
                response,
            })
            .await
            .map_err(|_| unavailable())?;
        tokio::time::timeout(COMMAND_TIMEOUT, result)
            .await
            .map_err(|_| timeout())?
            .map_err(|_| unavailable())?
    }

    pub async fn stop(&self) {
        self.cancellation.cancel();
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
    }

    pub async fn cancel_without_wait(&self) {
        self.cancellation.cancel();
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
        }
    }

    async fn initialize(&self, download_path: &Path) -> Result<(), BrowserError> {
        self.call(
            "Target.setDiscoverTargets",
            json!({ "discover": true }),
            None,
        )
        .await?;
        self.call(
            "Target.setAutoAttach",
            json!({ "autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true }),
            None,
        )
        .await?;
        self.call(
            "Browser.setDownloadBehavior",
            json!({
                "behavior": "allowAndName",
                "downloadPath": download_path.to_string_lossy(),
                "eventsEnabled": true
            }),
            None,
        )
        .await?;
        Ok(())
    }
}

async fn run_observer<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    mut commands: mpsc::Receiver<CdpRequest>,
    cancellation: CancellationToken,
    manager: BrowserSessionManager,
    generation: u64,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut sink, mut source) = socket.split();
    let mut pending = HashMap::new();
    let mut sessions = HashMap::new();
    let mut frames = HashMap::new();
    let mut next_id = 1_u64;
    let mut disconnect_reason = None;
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                let _ = send_with_timeout(
                    &mut sink, Message::Close(None), OBSERVER_CLOSE_TIMEOUT,
                ).await;
                break;
            }
            request = commands.recv() => {
                let Some(request) = request else { break; };
                let id = next_id;
                next_id = next_id.saturating_add(1);
                let message = command_message(id, &request);
                if send_with_timeout(
                    &mut sink,
                    Message::Text(message.to_string().into()),
                    SOCKET_WRITE_TIMEOUT,
                ).await.is_err() {
                    let _ = request.response.send(Err(unavailable()));
                    disconnect_reason = Some("socket_write_failed".to_string());
                    break;
                }
                pending.insert(id, request.response);
            }
            incoming = source.next() => {
                let Some(result) = incoming else {
                    disconnect_reason = Some("socket_eof".to_string());
                    break;
                };
                let Ok(message) = result else {
                    disconnect_reason = Some("socket_read_failed".to_string());
                    break;
                };
                if let Message::Text(text) = message {
                    if handle_message(
                        &text, &mut pending, &mut sessions, &mut frames,
                        &manager, generation, &mut sink, &mut next_id,
                    ).await.is_err() {
                        disconnect_reason = Some("event_dispatch_failed".to_string());
                        break;
                    }
                }
            }
        }
    }
    for (_, response) in pending {
        let _ = response.send(Err(unavailable()));
    }
    if let Some(reason) = disconnect_reason.filter(|_| !cancellation.is_cancelled()) {
        manager.handle_cdp_disconnect(generation, reason).await;
    }
}

fn command_message(id: u64, request: &CdpRequest) -> Value {
    let mut message = json!({
        "id": id,
        "method": request.method,
        "params": request.params,
    });
    if let Some(session_id) = &request.session_id {
        message["sessionId"] = Value::String(session_id.clone());
    }
    message
}

async fn handle_message<S>(
    text: &str,
    pending: &mut HashMap<u64, oneshot::Sender<Result<Value, BrowserError>>>,
    sessions: &mut HashMap<String, String>,
    frames: &mut HashMap<String, String>,
    manager: &BrowserSessionManager,
    generation: u64,
    sink: &mut S,
    next_id: &mut u64,
) -> Result<(), ()>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return Ok(());
    };
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        if let Some(response) = pending.remove(&id) {
            let result = if message.get("error").is_some() {
                Err(command_rejected())
            } else {
                Ok(message.get("result").cloned().unwrap_or(Value::Null))
            };
            let _ = response.send(result);
        }
        return Ok(());
    }
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Ok(());
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let session_id = message.get("sessionId").and_then(Value::as_str);
    update_protocol_maps(method, &params, session_id, sessions, frames);
    if method == "Target.attachedToTarget" {
        if let Some(session) = params.get("sessionId").and_then(Value::as_str) {
            enable_page_events(sink, next_id, session).await?;
        }
    }
    let target_id = session_id.and_then(|id| sessions.get(id)).cloned();
    let frame_target = params
        .get("frameId")
        .and_then(Value::as_str)
        .and_then(|id| frames.get(id))
        .cloned();
    manager
        .handle_cdp_event(
            generation,
            method,
            params,
            session_id,
            target_id,
            frame_target,
        )
        .await;
    Ok(())
}

async fn enable_page_events<S>(sink: &mut S, next_id: &mut u64, session_id: &str) -> Result<(), ()>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    for (method, params) in [
        ("Page.enable", json!({})),
        ("Page.setLifecycleEventsEnabled", json!({ "enabled": true })),
        (
            "Page.setInterceptFileChooserDialog",
            json!({ "enabled": true }),
        ),
    ] {
        let message = json!({
            "id": *next_id,
            "method": method,
            "params": params,
            "sessionId": session_id,
        });
        *next_id = (*next_id).saturating_add(1);
        send_with_timeout(
            sink,
            Message::Text(message.to_string().into()),
            SOCKET_WRITE_TIMEOUT,
        )
        .await?;
    }
    Ok(())
}
