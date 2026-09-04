//! ACP-compatible agent facade over the in-process Codex App Server.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sacp::{
    on_receive_dispatch, Agent, Client, ConnectTo, ConnectionTo, Dispatch, Handled, Responder,
    UntypedMessage,
};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::{
    CapabilitySet, ServerRequestTarget, SessionOwner, UpstreamClient, UpstreamError, UpstreamEvent,
    UpstreamEventPoll, UpstreamStartArgs,
};

mod acp_mapping;
mod item_mapping;
mod prompt_mapping;
mod settings_mapping;

#[derive(Debug, Clone)]
pub struct CodexAcpAgent {
    args: UpstreamStartArgs,
    owner: SessionOwner,
    expected_session_id: Option<String>,
}

impl CodexAcpAgent {
    pub fn new(args: UpstreamStartArgs) -> Result<Self, UpstreamError> {
        let owner = SessionOwner::new("codex-inprocess-bridge", None, 0)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        Ok(Self {
            args,
            owner,
            expected_session_id: None,
        })
    }

    pub fn with_owner(
        mut self,
        connection_id: impl Into<String>,
        conversation_id: Option<i32>,
        generation: u64,
    ) -> Result<Self, UpstreamError> {
        self.owner = SessionOwner::new(connection_id, conversation_id, generation)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        Ok(self)
    }

    pub fn with_expected_session_id(mut self, session_id: Option<String>) -> Self {
        self.expected_session_id = session_id;
        self
    }
}

impl ConnectTo<Client> for CodexAcpAgent {
    async fn connect_to(self, client: impl ConnectTo<Agent>) -> Result<(), sacp::Error> {
        let capabilities = self.args.capabilities;
        let owner = self.owner;
        let expected_cwd = self.args.cwd.clone();
        let expected_session_id = self.expected_session_id;
        let upstream = UpstreamClient::start(self.args)
            .await
            .map_err(to_sacp_error)?;
        let (command_tx, command_rx) = mpsc::channel(64);
        let state = BridgeState { command_tx };
        let bridge_command_tx = state.command_tx.clone();
        Agent
            .builder()
            .name("iyw-claw-codex-inprocess")
            .on_receive_dispatch(
                move |dispatch: Dispatch<UntypedMessage, UntypedMessage>, cx| {
                    let state = state.clone();
                    async move { dispatch_message(state, dispatch, cx).await }
                },
                on_receive_dispatch!(),
            )
            .connect_with(client, async move |cx| {
                let authority = BridgeAuthority {
                    owner,
                    capabilities,
                    expected_cwd,
                    expected_session_id,
                };
                run_bridge(upstream, command_rx, cx, bridge_command_tx, authority).await
            })
            .await
    }
}

#[derive(Clone)]
struct BridgeState {
    command_tx: mpsc::Sender<BridgeCommand>,
}

enum BridgeCommand {
    Request {
        method: String,
        params: Value,
        response: oneshot::Sender<Result<Value, String>>,
    },
    Prompt {
        params: Value,
        responder: Responder<Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerResponse {
        token: crate::ServerRequestToken,
        target: ServerRequestTarget,
        method: String,
        response: Result<Value, String>,
    },
}

struct PendingPrompt {
    thread_id: String,
    turn_id: String,
    responder: Responder<Value>,
}

struct BridgeAuthority {
    owner: SessionOwner,
    capabilities: CapabilitySet,
    expected_cwd: PathBuf,
    expected_session_id: Option<String>,
}

async fn dispatch_message(
    state: BridgeState,
    dispatch: Dispatch<UntypedMessage, UntypedMessage>,
    cx: ConnectionTo<Client>,
) -> Result<Handled<Dispatch<UntypedMessage, UntypedMessage>>, sacp::Error> {
    match dispatch {
        Dispatch::Request(request, responder) => {
            if request.method == "session/prompt" {
                state
                    .command_tx
                    .send(BridgeCommand::Prompt {
                        params: request.params,
                        responder,
                    })
                    .await
                    .map_err(|_| to_sacp_error("Codex bridge command channel closed"))?;
                return Ok(Handled::Yes);
            }
            let (response_tx, response_rx) = oneshot::channel();
            state
                .command_tx
                .send(BridgeCommand::Request {
                    method: request.method,
                    params: request.params,
                    response: response_tx,
                })
                .await
                .map_err(|_| to_sacp_error("Codex bridge command channel closed"))?;
            cx.spawn(async move {
                match response_rx.await {
                    Ok(Ok(value)) => responder.respond(value),
                    Ok(Err(error)) => responder.respond_with_error(to_sacp_error(error)),
                    Err(_) => {
                        responder.respond_with_error(to_sacp_error("Codex bridge response lost"))
                    }
                }
            })?;
            Ok(Handled::Yes)
        }
        Dispatch::Notification(notification) => {
            state
                .command_tx
                .send(BridgeCommand::Notification {
                    method: notification.method,
                    params: notification.params,
                })
                .await
                .map_err(|_| to_sacp_error("Codex bridge command channel closed"))?;
            Ok(Handled::Yes)
        }
        Dispatch::Response(response, router) => {
            router.respond_with_result(response)?;
            Ok(Handled::Yes)
        }
    }
}

async fn run_bridge(
    mut upstream: UpstreamClient,
    mut commands: mpsc::Receiver<BridgeCommand>,
    cx: ConnectionTo<Client>,
    command_tx: mpsc::Sender<BridgeCommand>,
    authority: BridgeAuthority,
) -> Result<(), sacp::Error> {
    let mut session_id = None;
    let mut pending_prompt = None;
    let mut item_projection = item_mapping::ItemProjection::default();
    let mut session_settings = settings_mapping::SessionSettings::default();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => {
                    if let Err(error) = handle_command(&mut upstream, command, &mut session_id, &mut pending_prompt, &authority, &mut session_settings).await {
                        reject_pending_prompt(&mut pending_prompt, &error);
                        return Err(error);
                    }
                }
                None => {
                    reject_pending_prompt(&mut pending_prompt, "Codex ACP client disconnected");
                    return upstream.shutdown().await.map_err(to_sacp_error);
                }
            },
            event = upstream.poll_event(Duration::from_millis(50)) => match event {
                Ok(UpstreamEventPoll::Event(event)) => {
                    if let Err(error) = handle_event(&upstream, *event, &cx, &command_tx, &session_id, &mut pending_prompt, &mut item_projection).await {
                        reject_pending_prompt(&mut pending_prompt, &error);
                        return Err(error);
                    }
                }
                Ok(UpstreamEventPoll::Timeout) => {}
                Ok(UpstreamEventPoll::Closed) => {
                    reject_pending_prompt(&mut pending_prompt, "Codex App Server closed before completing the turn");
                    return Ok(());
                }
                Err(error) => {
                    let error = to_sacp_error(error);
                    reject_pending_prompt(&mut pending_prompt, &error);
                    return Err(error);
                }
            },
        }
    }
}

fn reject_pending_prompt(pending_prompt: &mut Option<PendingPrompt>, message: impl ToString) {
    if let Some(pending) = pending_prompt.take() {
        let _ = pending.responder.respond_with_error(to_sacp_error(message));
    }
}

async fn handle_command(
    upstream: &mut UpstreamClient,
    command: BridgeCommand,
    session_id: &mut Option<String>,
    pending_prompt: &mut Option<PendingPrompt>,
    authority: &BridgeAuthority,
    session_settings: &mut settings_mapping::SessionSettings,
) -> Result<(), sacp::Error> {
    match command {
        BridgeCommand::Prompt { params, responder } => {
            match start_prompt(
                &mut *upstream,
                params,
                session_id,
                pending_prompt,
                authority.capabilities,
            )
            .await
            {
                Ok(turn_id) => {
                    *pending_prompt = Some(PendingPrompt {
                        thread_id: session_id.clone().unwrap_or_default(),
                        turn_id,
                        responder,
                    });
                }
                Err(error) => {
                    let _ = responder.respond_with_error(to_sacp_error(error));
                }
            }
        }
        BridgeCommand::Request {
            method,
            params,
            response,
        } => {
            let result = handle_request(
                upstream,
                &method,
                params,
                session_id,
                authority,
                session_settings,
            )
            .await
            .map_err(|error| error.to_string());
            let _ = response.send(result);
        }
        BridgeCommand::Notification { method, params } if method == "session/cancel" => {
            let Some(thread_id) = params.get("sessionId").and_then(Value::as_str) else {
                // Notifications have no response channel. A malformed or
                // stale cancel must not take down an otherwise healthy ACP
                // bridge, so treat it as an ignored cancellation.
                return Ok(());
            };
            if session_id.as_deref() != Some(thread_id) {
                return Ok(());
            }
            // Completion and cancellation can cross at the protocol boundary.
            // A cancel after completion is already settled is a no-op, not a
            // bridge failure that would tear down the connection.
            let has_active_turn = upstream.active_turn_for(thread_id).await.is_some();
            if has_active_turn {
                upstream
                    .interrupt_turn_for_thread(thread_id)
                    .await
                    .map_err(to_sacp_error)?;
            }
            if let Some(pending) = pending_prompt.take() {
                if pending.thread_id == thread_id {
                    let _ = pending
                        .responder
                        .respond(json!({ "stopReason": "cancelled" }));
                } else {
                    *pending_prompt = Some(pending);
                }
            }
            if has_active_turn {
                let Some(turn) = upstream.active_turn_for(thread_id).await else {
                    return Ok(());
                };
                let _ = upstream
                    .complete_turn_for_thread(thread_id, &turn.turn_id)
                    .await;
            }
        }
        BridgeCommand::Notification { .. } => {}
        BridgeCommand::ServerResponse {
            token,
            target,
            method,
            response,
        } => {
            resolve_server_response(upstream, token, target, &method, response).await?;
        }
    }
    Ok(())
}

async fn start_prompt(
    upstream: &mut UpstreamClient,
    params: Value,
    session_id: &mut Option<String>,
    pending_prompt: &mut Option<PendingPrompt>,
    capabilities: CapabilitySet,
) -> Result<String, UpstreamError> {
    let id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| UpstreamError::InvalidRequest("session/prompt has no sessionId".into()))?;
    if session_id.as_deref() != Some(id) {
        return Err(UpstreamError::InvalidRequest(
            "session/prompt session does not match the bound session".into(),
        ));
    }
    if pending_prompt.is_some() {
        return Err(UpstreamError::InvalidRequest(
            "a Codex prompt is already pending".into(),
        ));
    }
    let request = prompt_mapping::turn_start_request(&params, capabilities)?;
    let response = upstream.start_turn_for_thread(id, request).await?;
    let turn_id = response
        .pointer("/turn/id")
        .or_else(|| response.pointer("/turnId"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidResponse("turn response has no turn id".into()))?;
    *session_id = Some(id.to_string());
    Ok(turn_id)
}

async fn handle_request(
    upstream: &mut UpstreamClient,
    method: &str,
    params: Value,
    session_id: &mut Option<String>,
    authority: &BridgeAuthority,
    session_settings: &mut settings_mapping::SessionSettings,
) -> Result<Value, UpstreamError> {
    match method {
        "initialize" => Ok(acp_mapping::initialize_response(
            &params,
            authority.capabilities,
            authority.expected_session_id.is_some(),
        )),
        "session/new" => {
            validate_cwd(&params, &authority.expected_cwd)?;
            if authority.expected_session_id.is_some() {
                return Err(UpstreamError::InvalidRequest(
                    "session/new cannot replace the owning persisted session".into(),
                ));
            }
            ensure_session_slot(session_id, None)?;
            let request = acp_mapping::thread_start_request(&params)?;
            let response = upstream
                .start_thread(authority.owner.clone(), request, authority.capabilities)
                .await?;
            let id = crate::upstream_backend::thread_id_from_response_for_bridge(&response)?;
            session_settings.capture(&response);
            *session_id = Some(id.clone());
            Ok(settings_mapping::new_session_response(
                &id,
                session_settings,
            ))
        }
        "session/load" => {
            validate_cwd(&params, &authority.expected_cwd)?;
            let id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    UpstreamError::InvalidRequest("session/load has no sessionId".into())
                })?;
            if authority.expected_session_id.as_deref() != Some(id) {
                return Err(UpstreamError::InvalidRequest(
                    "session/load id does not match the owning persisted session".into(),
                ));
            }
            ensure_session_slot(session_id, Some(id))?;
            let request = acp_mapping::thread_resume_request(&params)?;
            let response = upstream
                .resume_thread(authority.owner.clone(), request, authority.capabilities)
                .await?;
            session_settings.capture(&response);
            *session_id = Some(id.to_string());
            Ok(settings_mapping::new_session_response(id, session_settings))
        }
        "thread/goal/set" | "thread/goal/get" | "thread/goal/clear" => {
            let id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    UpstreamError::InvalidRequest("goal request has no sessionId".into())
                })?;
            upstream
                .request_json_for_thread(id, acp_mapping::goal_request(method, &params)?)
                .await
        }
        "session/set_mode" | "session/set_config_option" => {
            let id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    UpstreamError::InvalidRequest("session request has no sessionId".into())
                })?;
            let (request, change) = settings_mapping::request(method, &params, session_settings)?;
            upstream.request_json_for_thread(id, request).await?;
            session_settings.apply(change);
            Ok(settings_mapping::response(method, session_settings))
        }
        _ => Err(UpstreamError::InvalidRequest(format!(
            "ACP method is not implemented by the in-process bridge: {method}"
        ))),
    }
}

fn ensure_session_slot(
    session_id: &Option<String>,
    requested: Option<&str>,
) -> Result<(), UpstreamError> {
    match (session_id.as_deref(), requested) {
        (None, _) => Ok(()),
        (Some(_), Some(id)) if session_id.as_deref() == Some(id) => Ok(()),
        (Some(_), _) => Err(UpstreamError::InvalidRequest(
            "Codex ACP connection already owns a different session".into(),
        )),
    }
}

async fn handle_event(
    upstream: &UpstreamClient,
    event: UpstreamEvent,
    cx: &ConnectionTo<Client>,
    command_tx: &mpsc::Sender<BridgeCommand>,
    session_id: &Option<String>,
    pending_prompt: &mut Option<PendingPrompt>,
    item_projection: &mut item_mapping::ItemProjection,
) -> Result<(), sacp::Error> {
    match event {
        UpstreamEvent::Lagged { skipped } => {
            if session_id.is_some() {
                send_update(
                    cx,
                    session_id,
                    "agent_message_chunk",
                    json!({
                        "content": { "type": "text", "text": format!("[Codex events skipped: {skipped}]") }
                    }),
                )?;
            }
        }
        UpstreamEvent::ServerRequest {
            admission,
            method,
            params,
            ..
        } => {
            if acp_mapping::is_permission_method(&method) {
                match forward_permission_request(
                    cx,
                    command_tx,
                    admission.clone(),
                    method.clone(),
                    params,
                ) {
                    Ok(()) => {}
                    Err(error) => {
                        reject_server_request(upstream, admission, method).await?;
                        let _ = error;
                    }
                }
            } else {
                reject_server_request(upstream, admission, method).await?;
            }
        }
        UpstreamEvent::ServerNotification { method, params } => {
            let thread_id = params
                .get("threadId")
                .and_then(Value::as_str)
                .or_else(|| params.get("thread_id").and_then(Value::as_str));
            if notification_requires_thread(&method) && thread_id.is_none() {
                return Ok(());
            }
            if let Some(thread_id) = thread_id {
                if session_id.as_deref() != Some(thread_id) {
                    return Ok(());
                }
            }
            if method == "turn/completed" {
                let thread_id = params
                    .get("threadId")
                    .or_else(|| params.get("thread_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| to_sacp_error("turn/completed has no thread id"))?;
                let turn_id = params
                    .pointer("/turn/id")
                    .or_else(|| params.pointer("/turnId"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| to_sacp_error("turn/completed has no turn id"))?;
                let pending_matches = pending_prompt.as_ref().is_some_and(|pending| {
                    pending.thread_id == thread_id && pending.turn_id == turn_id
                });
                // Ignore duplicate and stale completions. In particular, an
                // old completion must never clear or fail a newer active turn
                // on the same thread.
                let Some(active_turn) = upstream.active_turn_for(thread_id).await else {
                    return Ok(());
                };
                if active_turn.turn_id != turn_id {
                    return Ok(());
                }
                upstream
                    .complete_turn_for_thread(thread_id, turn_id)
                    .await
                    .map_err(to_sacp_error)?;
                if pending_matches {
                    if let Some(pending) = pending_prompt.take() {
                        let _ = pending
                            .responder
                            .respond(acp_mapping::prompt_response(&params));
                    }
                }
            } else if let Some(update) = acp_mapping::notification_to_update(&method, &params) {
                send_update(cx, session_id, update.method, update.params)?;
            } else if let Some(update) = item_projection.map(&method, &params) {
                send_update(cx, session_id, update.method, update.params)?;
            }
        }
    }
    let _ = upstream;
    Ok(())
}

fn notification_requires_thread(method: &str) -> bool {
    if method.starts_with("item/") {
        return true;
    }
    matches!(
        method,
        "item/agentMessage/delta"
            | "thread/name/updated"
            | "thread/goal/updated"
            | "thread/goal/cleared"
            | "turn/plan/updated"
            | "thread/tokenUsage/updated"
            | "turn/completed"
    )
}

async fn reject_server_request(
    upstream: &UpstreamClient,
    admission: crate::AdmittedServerRequest,
    method: String,
) -> Result<(), sacp::Error> {
    let message = format!("Codex server request is not yet mapped to the ACP authority: {method}");
    match admission.target {
        ServerRequestTarget::Global => upstream
            .reject_global_server_request(admission.token, -32601, message)
            .await
            .map_err(to_sacp_error),
        ServerRequestTarget::Session(binding) => {
            let access = crate::SessionAccess {
                external_id: &binding.external_id,
                connection_id: &binding.connection_id,
                generation: binding.generation,
                runtime_fingerprint: &binding.runtime_fingerprint,
            };
            upstream
                .reject_session_server_request(access, admission.token, -32601, message)
                .await
                .map_err(to_sacp_error)
        }
    }
}

fn forward_permission_request(
    cx: &ConnectionTo<Client>,
    command_tx: &mpsc::Sender<BridgeCommand>,
    admission: crate::AdmittedServerRequest,
    method: String,
    params: Value,
) -> Result<(), sacp::Error> {
    let request = acp_mapping::permission_request(&method, &params).map_err(to_sacp_error)?;
    let token = admission.token;
    let target = admission.target;
    let command_tx = command_tx.clone();
    let sent = cx.send_request_to(Client, request);
    cx.spawn(async move {
        let response = match sent.block_task().await {
            Ok(response) => serde_json::to_value(response).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let _ = command_tx
            .send(BridgeCommand::ServerResponse {
                token,
                target,
                method,
                response,
            })
            .await;
        Ok(())
    })
}

async fn resolve_server_response(
    upstream: &UpstreamClient,
    token: crate::ServerRequestToken,
    target: ServerRequestTarget,
    method: &str,
    response: Result<Value, String>,
) -> Result<(), sacp::Error> {
    let response = match response {
        Ok(value) => match acp_mapping::permission_decision(method, &value) {
            Ok(response) => response,
            Err(error) => {
                reject_admitted_request(upstream, token, target, method, error.to_string()).await?;
                return Ok(());
            }
        },
        Err(error) => {
            reject_admitted_request(
                upstream,
                token,
                target,
                method,
                format!("ACP permission request failed: {error}"),
            )
            .await?;
            return Ok(());
        }
    };
    match target {
        ServerRequestTarget::Global => upstream
            .resolve_global_server_request(token, response)
            .await
            .map_err(to_sacp_error),
        ServerRequestTarget::Session(binding) => {
            let access = crate::SessionAccess {
                external_id: &binding.external_id,
                connection_id: &binding.connection_id,
                generation: binding.generation,
                runtime_fingerprint: &binding.runtime_fingerprint,
            };
            upstream
                .resolve_session_server_request(access, token, response)
                .await
                .map_err(to_sacp_error)
        }
    }
}

async fn reject_admitted_request(
    upstream: &UpstreamClient,
    token: crate::ServerRequestToken,
    target: ServerRequestTarget,
    method: &str,
    reason: String,
) -> Result<(), sacp::Error> {
    reject_server_request(
        upstream,
        crate::AdmittedServerRequest {
            token,
            method: method.to_string(),
            class: crate::RequestClass::PermissionResponse,
            target,
            turn_id: None,
        },
        format!("{method}: {reason}"),
    )
    .await
}

fn send_update(
    cx: &ConnectionTo<Client>,
    session_id: &Option<String>,
    kind: &str,
    params: Value,
) -> Result<(), sacp::Error> {
    let id = session_id.clone().unwrap_or_else(|| "unknown".to_string());
    cx.send_notification(UntypedMessage::new(
        "session/update",
        json!({ "sessionId": id, "update": acp_mapping::update_payload(kind, params) }),
    )?)
}

fn to_sacp_error(error: impl ToString) -> sacp::Error {
    sacp::util::internal_error(error.to_string())
}

fn validate_cwd(params: &Value, expected: &Path) -> Result<(), UpstreamError> {
    let requested = params
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or_else(|| UpstreamError::InvalidRequest("ACP session request has no cwd".into()))?;
    let requested = std::fs::canonicalize(requested)
        .map_err(|error| UpstreamError::InvalidRequest(format!("invalid ACP cwd: {error}")))?;
    let expected = std::fs::canonicalize(expected).map_err(|error| {
        UpstreamError::Start(format!("configured Codex cwd is invalid: {error}"))
    })?;
    if requested != expected {
        return Err(UpstreamError::InvalidRequest(
            "ACP cwd does not match the owning Codex runtime".into(),
        ));
    }
    Ok(())
}
