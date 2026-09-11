//! ACP-compatible agent facade over the in-process Codex App Server.

use std::path::{Path, PathBuf};

use sacp::{
    on_receive_dispatch, Agent, Client, ConnectTo, ConnectionTo, Dispatch, Handled, Responder,
    UntypedMessage,
};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::{
    CapabilitySet, ServerRequestTarget, SessionOwner, UpstreamClient, UpstreamError, UpstreamEvent,
    UpstreamStartArgs,
};

mod acp_mapping;
mod activity_mapping;
mod approval_mapping;
mod automatic_turn;
mod child_events;
mod commands;
mod completed_snapshot;
mod fast_mode;
mod event_recovery;
mod message_projection;
mod interaction;
mod interaction_mapping;
mod interaction_registry;
mod item_mapping;
mod model_settings;
mod native_title;
mod permission_profile;
mod prompt_mapping;
mod session_options;
mod settings_mapping;
mod steering;
mod subagent_items;
mod tool_content;
mod thinking_projection;

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
            .name("iyw-claw-xinghe-inprocess")
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
                    client_form_supported: false,
                    session_launch: None,
                    interactions: Default::default(),
                    automatic: Default::default(),
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
    PublishCommands { session_id: String },
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
        turn_id: Option<String>,
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
    client_form_supported: bool,
    session_launch: Option<crate::upstream_mcp::ThreadLaunchOptions>,
    interactions: interaction_registry::InteractionRegistry,
    automatic: automatic_turn::AutomaticTurnState,
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
            let publish_commands = matches!(request.method.as_str(), "session/new" | "session/load" | "session/resume" | "session/fork");
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
                    Ok(Ok(value)) => {
                        let session_id = value["sessionId"].as_str().map(str::to_string);
                        responder.respond(value)?;
                        if let Some(session_id) = session_id.filter(|_| publish_commands) {
                            let _ = state.command_tx.send(BridgeCommand::PublishCommands { session_id }).await;
                        }
                        Ok(())
                    }
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
    commands: mpsc::Receiver<BridgeCommand>,
    cx: ConnectionTo<Client>,
    command_tx: mpsc::Sender<BridgeCommand>,
    authority: BridgeAuthority,
) -> Result<(), sacp::Error> {
    let result = run_bridge_loop(&mut upstream, commands, cx, command_tx, authority).await;
    let shutdown = upstream.shutdown().await.map_err(to_sacp_error);
    result.and(shutdown)
}

async fn run_bridge_loop(
    upstream: &mut UpstreamClient,
    mut commands: mpsc::Receiver<BridgeCommand>,
    cx: ConnectionTo<Client>,
    command_tx: mpsc::Sender<BridgeCommand>,
    mut authority: BridgeAuthority,
) -> Result<(), sacp::Error> {
    let mut session_id = None;
    let mut pending_prompt = None;
    let mut item_projection = item_mapping::ItemProjection::default();
    let mut session_settings = settings_mapping::SessionSettings::default();
    let mut messages = message_projection::MessageProjection::default();
    let mut thinking = thinking_projection::ThinkingProjection::default();
    let mut native_title = native_title::NativeTitle::default();
    loop {
        if authority.automatic.awaiting_prompt.is_none() {
            if let Some(command) = authority.automatic.deferred_prompt.take() {
                handle_command(upstream, command, &mut session_id, &mut pending_prompt, &mut authority, &mut session_settings, &cx).await?;
            }
        }
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => {
                    let session_request = matches!(&command, BridgeCommand::Request { method, .. }
                        if matches!(method.as_str(), "session/new" | "session/load" | "session/resume" | "session/fork"));
                    let title_prompt = match &command {
                        BridgeCommand::Prompt { params, .. } => native_title::prompt_text(params),
                        _ => None,
                    };
                    if let Err(error) = handle_command(upstream, command, &mut session_id, &mut pending_prompt, &mut authority, &mut session_settings, &cx).await {
                        reject_pending_prompt(&mut pending_prompt, &error);
                        return Err(error);
                    }
                    if session_request {
                        if let Some(title) = session_settings.native_title() {
                            send_update(&cx, &session_id, "session_info_update", json!({"title": title}))?;
                        }
                    }
                    if let (Some(prompt), Some(id), Some(_)) = (title_prompt, &session_id, &pending_prompt) {
                        native_title.start(upstream.native_title_handle(), native_title::TitleInput {
                            source_thread: id.clone(), prompt, model: session_settings.title_model(),
                            cwd: authority.expected_cwd.to_string_lossy().into_owned(),
                        });
                    }
                }
                None => {
                    reject_pending_prompt(&mut pending_prompt, "Codex ACP client disconnected");
                    return Ok(());
                }
            },
            event = upstream.receive_event() => {
                let Some(event) = event else {
                    reject_pending_prompt(&mut pending_prompt, "Codex App Server closed before completing the turn");
                    return Ok(());
                };
                let event = match upstream.convert_event(event).await {
                    Ok(Some(event)) => event,
                    Ok(None) => continue,
                    Err(error) => {
                        let error = to_sacp_error(error);
                        reject_pending_prompt(&mut pending_prompt, &error);
                        return Err(error);
                    }
                };
                    let events = match event {
                        UpstreamEvent::Lagged { skipped } => {
                            eprintln!("[星河][worker] recovering {skipped} missed runtime events");
                            match session_id.as_deref() {
                                Some(id) => event_recovery::recover(&upstream, id).await.map_err(to_sacp_error)?,
                                None => Vec::new(),
                            }
                        }
                        event => vec![event],
                    };
                    for event in events {
                    if native_title.route(&event) { continue; }
                    let context = BridgeEventContext {
                        upstream: &upstream,
                        cx: &cx,
                        command_tx: &command_tx,
                        session_id: &session_id,
                        pending_prompt: &mut pending_prompt,
                        item_projection: &mut item_projection,
                        settings: &mut session_settings,
                        client_form_supported: authority.client_form_supported,
                        interactions: &authority.interactions,
                        messages: &mut messages,
                        thinking: &mut thinking,
                        automatic: &mut authority.automatic,
                    };
                    if let Err(error) = handle_event(event, context).await {
                        reject_pending_prompt(&mut pending_prompt, &error);
                        return Err(error);
                    }
                }
            },
            _ = native_title.finished() => {},
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
    authority: &mut BridgeAuthority,
    session_settings: &mut settings_mapping::SessionSettings,
    cx: &ConnectionTo<Client>,
) -> Result<(), sacp::Error> {
    match command {
        BridgeCommand::PublishCommands { session_id: id } => {
            if session_id.as_deref() == Some(id.as_str()) {
                if commands::publish(upstream, cx, (&id, &authority.expected_cwd)).await.is_err() {
                    eprintln!("[星河][worker] available command publication failed");
                }
            }
        }
        BridgeCommand::Prompt { params, responder } => {
            authority.automatic.observe_prompt(&params, session_id.as_deref());
            if authority.automatic.awaiting_prompt.is_some() && commands::command(&params).is_some() {
                if authority.automatic.deferred_prompt.is_some() {
                    let _ = responder.respond_with_error(to_sacp_error("another user prompt is already waiting"));
                    return Ok(());
                }
                let thread = session_id.as_deref().ok_or_else(|| to_sacp_error("prompt has no session"))?;
                if let Err(error) = upstream.interrupt_turn_for_thread(thread).await {
                    if !matches!(&error, UpstreamError::Rpc { code: -32600, message } if message == "no active turn to interrupt") {
                        let _ = responder.respond_with_error(to_sacp_error(error));
                        return Ok(());
                    }
                }
                authority.automatic.deferred_prompt = Some(BridgeCommand::Prompt { params, responder });
                return Ok(());
            }
            let command = commands::handle(&params, commands::CommandContext {
                upstream, cx, session: session_id.as_deref(), settings: session_settings, cwd: &authority.expected_cwd,
            }).await;
            match command {
                Ok(true) => { let _ = responder.respond(json!({ "stopReason": "end_turn" })); return Ok(()); }
                Err(error) => { let _ = responder.respond_with_error(to_sacp_error(error)); return Ok(()); }
                Ok(false) => {}
            }
            match start_prompt(
                &mut *upstream,
                params,
                session_id,
                pending_prompt,
                session_settings.prompt_capabilities(authority.capabilities),
                authority.automatic.awaiting_prompt.is_some(),
            )
            .await
            {
                Ok(turn_id) => {
                    authority.automatic.awaiting_prompt = None;
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
            let automatic = pending_prompt.is_none() && authority.automatic.awaiting_prompt.is_none();
            if let Some(BridgeCommand::Prompt { responder, .. }) = authority.automatic.deferred_prompt.take() {
                let _ = responder.respond(json!({ "stopReason": "cancelled" }));
            }
            let has_active_turn = upstream.active_turn_for(thread_id).await.is_some();
            authority.interactions.cancel_session(cx, thread_id)?;
            upstream.cancel_owned_tree(thread_id).await.map_err(to_sacp_error)?;
            if let Some(pending) = pending_prompt.take() {
                if pending.thread_id == thread_id {
                    let _ = pending
                        .responder
                        .respond(json!({ "stopReason": "cancelled" }));
                } else {
                    *pending_prompt = Some(pending);
                }
            }
            if has_active_turn && automatic {
                send_update(cx, session_id, "session_info_update", automatic_turn::completed(&json!({ "turn": { "status": "interrupted" } })))?;
            }
            authority.automatic.awaiting_prompt = None;
        }
        BridgeCommand::Notification { .. } => {}
        BridgeCommand::ServerResponse {
            token,
            target,
            method,
            turn_id,
            response,
        } => {
            resolve_server_response(upstream, token, target, &method, turn_id, response).await?;
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
    adopt_automatic: bool,
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
    let response = if adopt_automatic {
        upstream.submit_queued_prompt(id, request).await?
    } else { upstream.start_turn_for_thread(id, request).await? };
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
    authority: &mut BridgeAuthority,
    session_settings: &mut settings_mapping::SessionSettings,
) -> Result<Value, UpstreamError> {
    match method {
        "initialize" => {
            authority.client_form_supported = params.pointer("/clientCapabilities/elicitation/form")
                .is_some_and(Value::is_object);
            Ok(acp_mapping::initialize_response(
                &params, authority.capabilities, authority.expected_session_id.is_some(),
            ))
        }
        "_iyw/worker/bind_owner" => {
            if session_id.is_some() { return Err(UpstreamError::InvalidRequest("cannot change the owner of a live worker session".into())); }
            let owner = params["connectionId"].as_str().ok_or_else(|| UpstreamError::InvalidRequest("worker owner is missing".into()))?;
            if authority.owner.connection_id != owner && authority.owner.connection_id != "runtime-host-prewarm" {
                return Err(UpstreamError::InvalidRequest("worker is bound to a different connection".into()));
            }
            authority.owner = SessionOwner::new(owner, None, 0).map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
            Ok(json!({ "bound": true }))
        }
        "session/new" => {
            validate_cwd(&params, &authority.expected_cwd)?;
            if authority.expected_session_id.is_some() {
                return Err(UpstreamError::InvalidRequest(
                    "session/new cannot replace the owning persisted session".into(),
                ));
            }
            ensure_session_slot(session_id, None)?;
            let request = acp_mapping::thread_start_request(&params)?;
            let options = crate::upstream_mcp::ThreadLaunchOptions::from_acp(&params, authority.capabilities)?;
            session_settings.load_models(upstream).await?;
            let response = upstream
                .start_configured_thread(authority.owner.clone(), request, options.clone())
                .await?;
            let id = crate::upstream_backend::thread_id_from_response_for_bridge(&response)?;
            crate::mcp_readiness::verify(upstream, &id, &options.mcp_names()).await?;
            authority.session_launch = Some(options);
            session_settings.capture(&response);
            *session_id = Some(id.clone());
            Ok(settings_mapping::new_session_response(
                &id,
                session_settings,
            ))
        }
        "session/load" | "session/resume" => {
            validate_cwd(&params, &authority.expected_cwd)?;
            let id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    UpstreamError::InvalidRequest("session recovery has no sessionId".into())
                })?;
            if authority.expected_session_id.as_deref() != Some(id) {
                return Err(UpstreamError::InvalidRequest(
                    "session recovery id does not match the owning persisted session".into(),
                ));
            }
            ensure_session_slot(session_id, Some(id))?;
            let request = acp_mapping::thread_resume_request(&params)?;
            let options = crate::upstream_mcp::ThreadLaunchOptions::from_acp(&params, authority.capabilities)?;
            session_settings.load_models(upstream).await?;
            let response = upstream
                .resume_configured_thread(authority.owner.clone(), request, options.clone())
                .await?;
            crate::mcp_readiness::verify(upstream, id, &options.mcp_names()).await?;
            authority.session_launch = Some(options);
            session_settings.capture(&response);
            *session_id = Some(id.to_string());
            Ok(settings_mapping::new_session_response(id, session_settings))
        }
        "session/fork" => {
            validate_cwd(&params, &authority.expected_cwd)?;
            let source = params.get("sessionId").and_then(Value::as_str)
                .filter(|id| session_id.as_deref() == Some(*id))
                .ok_or_else(|| UpstreamError::InvalidRequest("fork source does not match the bound session".into()))?;
            let options = authority.session_launch.clone()
                .ok_or_else(|| UpstreamError::InvalidRequest("fork has no owning launch configuration".into()))?
                .with_fork_settings(session_settings.fork_values())?;
            let request = json!({ "method": "thread/fork", "params": {
                "threadId": source, "excludeTurns": true, "deferGoalContinuation": true,
            } });
            let response = upstream.fork_configured_thread(request, options).await?;
            let id = crate::upstream_backend::thread_id_from_response_for_bridge(&response)?;
            session_settings.capture(&response);
            authority.expected_session_id = Some(id.clone());
            *session_id = Some(id.clone());
            Ok(settings_mapping::new_session_response(&id, session_settings))
        }
        "_session/steering" => {
            steering::handle(upstream, &params, (session_id.as_deref(), session_settings.prompt_capabilities(authority.capabilities))).await
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
        "session/set_mode" | "session/set_config_option" | "session/set_model" => {
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

struct BridgeEventContext<'a> {
    upstream: &'a UpstreamClient,
    cx: &'a ConnectionTo<Client>,
    command_tx: &'a mpsc::Sender<BridgeCommand>,
    session_id: &'a Option<String>,
    pending_prompt: &'a mut Option<PendingPrompt>,
    item_projection: &'a mut item_mapping::ItemProjection,
    settings: &'a mut settings_mapping::SessionSettings,
    client_form_supported: bool,
    interactions: &'a interaction_registry::InteractionRegistry,
    messages: &'a mut message_projection::MessageProjection,
    thinking: &'a mut thinking_projection::ThinkingProjection,
    automatic: &'a mut automatic_turn::AutomaticTurnState,
}

async fn handle_event(event: UpstreamEvent, context: BridgeEventContext<'_>) -> Result<(), sacp::Error> {
    let BridgeEventContext { upstream, cx, command_tx, session_id, pending_prompt, item_projection, settings, client_form_supported, interactions, messages, thinking, automatic } = context;
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
            id,
            admission,
            method,
            mut params,
            ..
        } => {
            if let ServerRequestTarget::Session(binding) = &admission.target {
                let root = session_id.as_deref().ok_or_else(|| to_sacp_error("server request has no root session"))?;
                if binding.external_id != root {
                    if !upstream.descendant_of(&binding.external_id, root).await {
                        return reject_server_request(upstream, admission, method).await;
                    }
                    params["threadId"] = json!(root);
                }
            }
            if interaction::is_method(&method) {
                if let Err(error) = interaction::forward(cx, command_tx, interaction::InteractionRequest {
                    admission: admission.clone(), params, form_supported: client_form_supported,
                    request_id: id.to_string(), registry: interactions.clone(),
                }) {
                    reject_server_request(upstream, admission, method).await?;
                    let _ = error;
                }
            } else if acp_mapping::is_permission_method(&method) {
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
            if method == "turn/completed" {
                // 完成通知必达；即使单独的 resolved 通知丢失，也撤回本轮残留交互。
                interactions.complete_turn(cx, &params)?;
            }
            if method == "serverRequest/resolved" {
                if let Some(id) = params.get("requestId") { interactions.resolved(cx, &id.to_string())?; }
                return Ok(());
            }
            if upstream.discover_subagents(&method, &params).await.is_err() {
                eprintln!("[星河][worker] subagent discovery failed; unverified child requests remain blocked");
            }
            if matches!(method.as_str(), "turn/started" | "turn/completed") {
                if let Some(thread) = params.get("threadId").and_then(Value::as_str) {
                    if upstream.bind_descendant(thread).await.is_err() { return Ok(()); }
                }
            }
            if !upstream.accepts_turn_event(&method, &params).await.map_err(to_sacp_error)? { return Ok(()); }
            let thread_id = params
                .get("threadId")
                .and_then(Value::as_str)
                .or_else(|| params.get("thread_id").and_then(Value::as_str));
            if notification_requires_thread(&method) && thread_id.is_none() {
                return Ok(());
            }
            if let Some(thread_id) = thread_id {
                if session_id.as_deref() != Some(thread_id) {
                    if let Some(root) = session_id.as_deref() {
                        if upstream.descendant_of(thread_id, root).await {
                            child_events::handle(upstream, &method, &params).await.map_err(to_sacp_error)?;
                        }
                    }
                    return Ok(());
                }
            }
            if matches!(method.as_str(), "turn/started" | "turn/completed") && pending_prompt.is_none() {
                let thread = params.get("threadId").and_then(Value::as_str).ok_or_else(|| to_sacp_error("automatic turn has no thread"))?;
                let turn = params.pointer("/turn/id").and_then(Value::as_str).ok_or_else(|| to_sacp_error("automatic turn has no id"))?;
                if !automatic.owns_turn(turn) { automatic.begin(cx, upstream, (thread, turn)).await?; }
            }
            if let Some(waiting) = automatic.awaiting_prompt.as_deref() {
                if method == "turn/completed" && params.pointer("/turn/id").and_then(Value::as_str) == Some(waiting) {
                    upstream.complete_turn_for_thread(session_id.as_deref().unwrap_or_default(), waiting).await.map_err(to_sacp_error)?;
                    automatic.awaiting_prompt = None;
                }
                // 等待已接受的用户 Prompt 接管输出，不将自动回合记成该用户输入的完成。
                return Ok(());
            }
            if method == "turn/completed" {
                if !completed_snapshot::reconcile(upstream, cx, (&params, item_projection, automatic.generation())).await? {
                    if let Some(update) = messages.map(&method, &params).map_err(to_sacp_error)? {
                        send_update(cx, session_id, update.method, update.params)?;
                    }
                }
            } else if let Some(update) = messages.map(&method, &params).map_err(to_sacp_error)? {
                send_update(cx, session_id, update.method, update.params)?;
            }
            if method == "item/agentMessage/delta" { return Ok(()); }
            if let Some(update) = thinking.map(&method, &params).map_err(to_sacp_error)? {
                send_update(cx, session_id, update.method, update.params)?;
            }
            if thinking_projection::is_delta(&method) { return Ok(()); }
            if method == "thread/settings/updated" {
                let snapshot = params.get("threadSettings").ok_or_else(|| {
                    to_sacp_error("thread/settings/updated has no settings")
                })?;
                settings.capture(snapshot);
                return send_update(cx, session_id, "config_option_update", json!({
                    "configOptions": settings_mapping::config_options(settings),
                }));
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
                        let response = match acp_mapping::prompt_failure(&params) {
                            Some(error) => Err(to_sacp_error(error)),
                            None => Ok(acp_mapping::prompt_response(&params)),
                        };
                        let _ = pending.responder.respond_with_result(response);
                    }
                } else {
                    send_update(cx, session_id, "session_info_update", automatic_turn::completed(&params))?;
                }
            } else if let Some(update) = activity_mapping::observation(&method, &params) {
                let active = upstream.active_turn_for(thread_id.unwrap_or_default()).await;
                if active.as_ref().is_some_and(|turn| {
                    params.get("turnId").and_then(Value::as_str) == Some(turn.turn_id.as_str())
                }) {
                    send_update(cx, session_id, update.method, update.params)?;
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
            | "thread/settings/updated"
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
    let turn_id = admission.turn_id;
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
                turn_id,
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
    turn_id: Option<String>,
    response: Result<Value, String>,
) -> Result<(), sacp::Error> {
    if !upstream.has_server_request(token).await {
        return Ok(());
    }
    if let (ServerRequestTarget::Session(binding), Some(expected)) = (&target, turn_id.as_deref()) {
        let current = upstream.active_turn_for(&binding.external_id).await;
        if !current.is_some_and(|turn| turn.turn_id == expected && !turn.cancelling) {
            reject_admitted_request(upstream, token, target, method, "interaction turn is no longer active".into()).await?;
            return Ok(());
        }
    }
    let response = match response {
        Ok(value) => match if interaction::is_method(method) { Ok(value) } else { acp_mapping::permission_decision(method, &value) } {
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
                format!("ACP interaction request failed: {error}"),
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
    sacp::util::internal_error(crate::diagnostics::safe_detail(&error.to_string()))
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
