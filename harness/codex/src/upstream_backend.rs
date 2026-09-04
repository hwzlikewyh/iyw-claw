//! Private adapter over Codex's typed in-process App Server client.

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicI64, Ordering};

use codex_app_server_client::{InProcessAppServerClient, InProcessAppServerRequestHandle};
use codex_app_server_protocol::{ClientRequest, RequestId};
use serde_json::{json, Value};

use crate::contracts::{CapabilitySet, SessionAccess, SessionBinding, SessionOwner};
use crate::runtime::{CodexHarness, HarnessError};
use crate::sessions::{ActiveTurn, SessionError, TurnBinding};
use crate::{
    client_method_policy, AdmittedServerRequest, MethodScope, ServerRequestAdmissionError,
    ServerRequestDescriptor, ServerRequestToken, TurnScope,
};

pub struct UpstreamClient {
    request_handle: InProcessAppServerRequestHandle,
    event_client: tokio::sync::Mutex<InProcessAppServerClient>,
    harness: tokio::sync::Mutex<CodexHarness>,
    runtime_fingerprint: String,
    next_request_id: AtomicI64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpstreamEvent {
    Lagged {
        skipped: usize,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
        admission: AdmittedServerRequest,
    },
    ServerNotification {
        method: String,
        params: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpstreamEventPoll {
    Timeout,
    Closed,
    Event(Box<UpstreamEvent>),
}

#[derive(Debug)]
pub enum UpstreamError {
    InvalidRequest(String),
    InvalidResponse(String),
    Start(String),
    Harness(HarnessError),
    Io(String),
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid Codex request: {message}"),
            Self::InvalidResponse(message) => {
                write!(formatter, "invalid Codex response: {message}")
            }
            Self::Start(message) => write!(formatter, "Codex runtime start failed: {message}"),
            Self::Harness(error) => error.fmt(formatter),
            Self::Io(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for UpstreamError {}

impl From<HarnessError> for UpstreamError {
    fn from(error: HarnessError) -> Self {
        Self::Harness(error)
    }
}

impl From<SessionError> for UpstreamError {
    fn from(error: SessionError) -> Self {
        Self::Harness(HarnessError::from(error))
    }
}

impl From<ServerRequestAdmissionError> for UpstreamError {
    fn from(error: ServerRequestAdmissionError) -> Self {
        match error {
            ServerRequestAdmissionError::Harness(error) => Self::Harness(error),
            error => Self::InvalidRequest(error.to_string()),
        }
    }
}

impl UpstreamClient {
    pub async fn start(args: crate::UpstreamStartArgs) -> Result<Self, UpstreamError> {
        if args.runtime_fingerprint.trim().is_empty() {
            return Err(UpstreamError::Start(
                "runtime fingerprint must not be empty".to_string(),
            ));
        }
        let runtime_fingerprint = args.runtime_fingerprint.clone();
        let capabilities = args.capabilities;
        let mut harness = CodexHarness::new(args.harness.clone())
            .map_err(|error| UpstreamError::Io(error.to_string()))?;
        let client = args.build_client().await?;
        let request_handle = client.request_handle();
        harness
            .mark_ready(capabilities)
            .map_err(|error| UpstreamError::Io(error.to_string()))?;
        Ok(Self {
            request_handle,
            event_client: tokio::sync::Mutex::new(client),
            harness: tokio::sync::Mutex::new(harness),
            runtime_fingerprint,
            next_request_id: AtomicI64::new(1),
        })
    }

    pub async fn start_thread(
        &self,
        owner: SessionOwner,
        request: Value,
        capabilities: CapabilitySet,
    ) -> Result<Value, UpstreamError> {
        ensure_method(&request, "thread/start")?;
        reject_unmanaged_thread_overrides(&request)?;
        owner
            .validate()
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        self.harness
            .lock()
            .await
            .validate_session_capabilities(capabilities)?;
        let response = self.send(request).await?;
        let thread_id = thread_id_from_response(&response)?;
        self.harness.lock().await.bind_session(
            SessionBinding::new(owner, thread_id, self.runtime_fingerprint.clone())
                .map_err(|error| UpstreamError::Io(error.to_string()))?,
            capabilities,
        )?;
        Ok(response)
    }

    pub async fn resume_thread(
        &self,
        owner: SessionOwner,
        request: Value,
        capabilities: CapabilitySet,
    ) -> Result<Value, UpstreamError> {
        ensure_method(&request, "thread/resume")?;
        reject_unmanaged_thread_overrides(&request)?;
        owner
            .validate()
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        self.harness
            .lock()
            .await
            .validate_session_capabilities(capabilities)?;
        let thread_id = thread_id_from_params(&request)?;
        let binding =
            SessionBinding::new(owner, thread_id.clone(), self.runtime_fingerprint.clone())
                .map_err(|error| UpstreamError::Io(error.to_string()))?;
        self.harness
            .lock()
            .await
            .validate_session_binding(&binding, capabilities)?;
        let response = self.send(request).await?;
        let response_thread_id = thread_id_from_response(&response)?;
        if response_thread_id != thread_id {
            return Err(UpstreamError::InvalidResponse(
                "thread/resume returned a different thread id".to_string(),
            ));
        }
        self.harness
            .lock()
            .await
            .bind_session(binding, capabilities)?;
        Ok(response)
    }

    pub async fn start_turn(
        &self,
        access: SessionAccess<'_>,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        ensure_method(&request, "turn/start")?;
        reject_unmanaged_turn_overrides(&request)?;
        let thread_id = thread_id_from_params(&request)?;
        self.harness.lock().await.ensure_can_begin_turn(access)?;
        let response = self.send_scoped(access, request).await?;
        let turn_id = turn_id_from_response(&response)?;
        if thread_id != access.external_id {
            return Err(UpstreamError::InvalidResponse(
                "turn/start thread id is not bound to the session".into(),
            ));
        }
        self.harness
            .lock()
            .await
            .begin_turn(access, TurnBinding::new(thread_id, turn_id)?)?;
        Ok(response)
    }

    pub async fn steer_turn(
        &self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        ensure_method(&request, "turn/steer")?;
        ensure_expected_turn_id(&request, expected_turn_id)?;
        self.harness
            .lock()
            .await
            .steer_turn(access, expected_turn_id)?;
        self.send_scoped(access, request).await
    }

    pub async fn interrupt_turn(
        &self,
        access: SessionAccess<'_>,
        expected_turn_id: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        ensure_method(&request, "turn/interrupt")?;
        ensure_turn_id(&request, expected_turn_id)?;
        self.harness
            .lock()
            .await
            .cancel_turn(access, expected_turn_id)?;
        self.send_scoped(access, request).await
    }

    pub async fn request_json(
        &self,
        access: SessionAccess<'_>,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        self.send_scoped(access, request).await
    }

    pub async fn request_global_json(&self, request: Value) -> Result<Value, UpstreamError> {
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| UpstreamError::InvalidRequest("request has no method".into()))?;
        let policy = client_method_policy(method)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        if policy.scope != MethodScope::Global {
            return Err(UpstreamError::InvalidRequest(
                "request requires a session binding".into(),
            ));
        }
        if let Some(capability) = policy.capability {
            self.harness
                .lock()
                .await
                .validate_runtime_capability(capability)?;
        }
        self.send(request).await
    }

    pub async fn next_event(&self) -> Option<UpstreamEvent> {
        loop {
            let event = {
                let mut event_client = self.event_client.lock().await;
                event_client.next_event().await?
            };
            match self.convert_event(event).await {
                Ok(Some(event)) => return Some(event),
                Ok(None) => continue,
                Err(_) => return None,
            }
        }
    }

    pub async fn poll_event(
        &self,
        wait: std::time::Duration,
    ) -> Result<UpstreamEventPoll, UpstreamError> {
        let event = match tokio::time::timeout(wait, async {
            let mut event_client = self.event_client.lock().await;
            event_client.next_event().await
        })
        .await
        {
            Ok(event) => event,
            Err(_) => return Ok(UpstreamEventPoll::Timeout),
        };
        let Some(event) = event else {
            return Ok(UpstreamEventPoll::Closed);
        };
        Ok(match self.convert_event(event).await? {
            Some(event) => UpstreamEventPoll::Event(Box::new(event)),
            None => UpstreamEventPoll::Timeout,
        })
    }

    async fn convert_event(
        &self,
        event: codex_app_server_client::InProcessServerEvent,
    ) -> Result<Option<UpstreamEvent>, UpstreamError> {
        let value = match event {
            codex_app_server_client::InProcessServerEvent::Lagged { skipped } => {
                return Ok(Some(UpstreamEvent::Lagged { skipped }));
            }
            codex_app_server_client::InProcessServerEvent::ServerRequest(request) => {
                let value = serde_json::to_value(request)
                    .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?;
                let id = value.get("id").cloned().ok_or_else(|| {
                    UpstreamError::InvalidResponse("server request has no id".into())
                })?;
                let method = value
                    .get("method")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        UpstreamError::InvalidResponse("server request has no method".into())
                    })?
                    .to_string();
                let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
                let descriptor =
                    server_request_descriptor(&id, &method, &params).ok_or_else(|| {
                        UpstreamError::InvalidResponse("server request id is invalid".into())
                    })?;
                let admission = match self.harness.lock().await.admit_server_request(descriptor) {
                    Ok(admission) => admission,
                    Err(error) => {
                        let _ = self
                            .reject_raw_server_request(id, -32602, error.to_string())
                            .await;
                        return Ok(None);
                    }
                };
                return Ok(Some(UpstreamEvent::ServerRequest {
                    id,
                    method,
                    params,
                    admission,
                }));
            }
            codex_app_server_client::InProcessServerEvent::ServerNotification(notification) => {
                serde_json::to_value(notification)
                    .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?
            }
        };
        Ok(event_from_json(value))
    }

    pub async fn resolve_global_server_request(
        &self,
        token: ServerRequestToken,
        result: Value,
    ) -> Result<(), UpstreamError> {
        let id = self
            .harness
            .lock()
            .await
            .take_global_server_request(token)?;
        let id: Value = serde_json::from_str(&id)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        let id: RequestId = serde_json::from_value(id)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        self.event_client
            .lock()
            .await
            .resolve_server_request(id, result)
            .await
            .map_err(io_error)
    }

    pub async fn resolve_session_server_request(
        &self,
        access: SessionAccess<'_>,
        token: ServerRequestToken,
        result: Value,
    ) -> Result<(), UpstreamError> {
        let id = self
            .harness
            .lock()
            .await
            .take_session_server_request(access, token)?;
        let id: Value = serde_json::from_str(&id)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        let id: RequestId = serde_json::from_value(id)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        self.event_client
            .lock()
            .await
            .resolve_server_request(id, result)
            .await
            .map_err(io_error)
    }

    pub async fn reject_global_server_request(
        &self,
        token: ServerRequestToken,
        code: i64,
        message: impl Into<String>,
    ) -> Result<(), UpstreamError> {
        let id = self
            .harness
            .lock()
            .await
            .take_global_server_request(token)?;
        self.reject_raw_server_request(
            serde_json::from_str(&id)
                .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?,
            code,
            message,
        )
        .await
    }

    pub async fn reject_session_server_request(
        &self,
        access: SessionAccess<'_>,
        token: ServerRequestToken,
        code: i64,
        message: impl Into<String>,
    ) -> Result<(), UpstreamError> {
        let id = self
            .harness
            .lock()
            .await
            .take_session_server_request(access, token)?;
        self.reject_raw_server_request(
            serde_json::from_str(&id)
                .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?,
            code,
            message,
        )
        .await
    }

    async fn reject_raw_server_request(
        &self,
        id: Value,
        code: i64,
        message: impl Into<String>,
    ) -> Result<(), UpstreamError> {
        let id: RequestId = serde_json::from_value(id)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        self.event_client
            .lock()
            .await
            .reject_server_request(
                id,
                codex_app_server_protocol::JSONRPCErrorError {
                    code,
                    data: None,
                    message: message.into(),
                },
            )
            .await
            .map_err(io_error)
    }

    pub async fn complete_turn(
        &self,
        access: SessionAccess<'_>,
        turn_id: &str,
    ) -> Result<ActiveTurn, UpstreamError> {
        self.harness
            .lock()
            .await
            .complete_turn(access, turn_id)
            .map_err(Into::into)
    }

    pub async fn shutdown(self) -> Result<(), UpstreamError> {
        self.harness
            .lock()
            .await
            .begin_shutdown()
            .map_err(|error| UpstreamError::Io(error.to_string()))?;
        self.event_client
            .into_inner()
            .shutdown()
            .await
            .map_err(io_error)?;
        self.harness
            .into_inner()
            .finish_shutdown()
            .map_err(|error| UpstreamError::Io(error.to_string()))
    }

    pub async fn binding_for(&self, external_id: &str) -> Option<SessionBinding> {
        self.harness.lock().await.binding(external_id)
    }

    pub async fn active_turn_for(&self, external_id: &str) -> Option<ActiveTurn> {
        self.harness.lock().await.active_turn_for(external_id)
    }

    pub async fn start_turn_for_thread(
        &self,
        thread_id: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        let binding = self
            .binding_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("unknown Codex thread".into()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        self.start_turn(access, request).await
    }

    pub async fn request_json_for_thread(
        &self,
        thread_id: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        let binding = self
            .binding_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("unknown Codex thread".into()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        self.request_json(access, request).await
    }

    pub async fn steer_turn_for_thread(
        &self,
        thread_id: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        let turn = self
            .active_turn_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("no active Codex turn".into()))?;
        let binding = self
            .binding_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("unknown Codex thread".into()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        self.steer_turn(access, &turn.turn_id, request).await
    }

    pub async fn interrupt_turn_for_thread(&self, thread_id: &str) -> Result<Value, UpstreamError> {
        let turn = self
            .active_turn_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("no active Codex turn".into()))?;
        let binding = self
            .binding_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("unknown Codex thread".into()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        self.interrupt_turn(
            access,
            &turn.turn_id,
            json!({
                "method": "turn/interrupt",
                "params": { "threadId": thread_id, "turnId": turn.turn_id }
            }),
        )
        .await
    }

    pub async fn complete_turn_for_thread(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<ActiveTurn, UpstreamError> {
        let binding = self
            .binding_for(thread_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("unknown Codex thread".into()))?;
        let access = SessionAccess {
            external_id: &binding.external_id,
            connection_id: &binding.connection_id,
            generation: binding.generation,
            runtime_fingerprint: &binding.runtime_fingerprint,
        };
        self.complete_turn(access, turn_id).await
    }

    async fn send_scoped(
        &self,
        access: SessionAccess<'_>,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| UpstreamError::InvalidRequest("request has no method".into()))?;
        let policy = client_method_policy(method)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        if policy.scope != MethodScope::Session {
            return Err(UpstreamError::InvalidRequest(
                "request does not accept a session binding".into(),
            ));
        }
        if method == "turn/start" {
            reject_unmanaged_turn_overrides(&request)?;
        }
        let thread_id = thread_id_from_params(&request)?;
        if thread_id != access.external_id {
            return Err(UpstreamError::InvalidRequest(
                "request thread id does not match session binding".into(),
            ));
        }
        self.harness.lock().await.validate_session(access)?;
        if let Some(capability) = policy.capability {
            self.harness
                .lock()
                .await
                .validate_capability(access, capability)?;
        }
        if matches!(policy.turn, TurnScope::Required) {
            let turn_id = active_turn_id_from_params(method, &request)?;
            self.harness.lock().await.validate_turn(access, &turn_id)?;
        }
        self.send(request).await
    }

    async fn send(&self, mut request: Value) -> Result<Value, UpstreamError> {
        let object = request.as_object_mut().ok_or_else(|| {
            UpstreamError::InvalidRequest("Codex request must be a JSON object".into())
        })?;
        object.insert(
            "id".to_string(),
            json!(self.next_request_id.fetch_add(1, Ordering::Relaxed)),
        );
        let request: ClientRequest = serde_json::from_value(request)
            .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        let result = self
            .request_handle
            .request(request)
            .await
            .map_err(io_error)?;
        result.map_err(|error| UpstreamError::Io(error.message))
    }
}

fn ensure_method(value: &Value, expected: &str) -> Result<(), UpstreamError> {
    if value.get("method").and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(UpstreamError::InvalidRequest(format!(
            "expected method {expected}"
        )))
    }
}

fn reject_unmanaged_thread_overrides(request: &Value) -> Result<(), UpstreamError> {
    reject_unmanaged_params(
        request,
        &[
            "model",
            "modelProvider",
            "model_provider",
            "serviceTier",
            "service_tier",
            "cwd",
            "runtimeWorkspaceRoots",
            "runtime_workspace_roots",
            "approvalPolicy",
            "approval_policy",
            "approvalsReviewer",
            "approvals_reviewer",
            "sandbox",
            "permissions",
            "config",
            "baseInstructions",
            "base_instructions",
            "developerInstructions",
            "developer_instructions",
            "personality",
            "serviceName",
            "service_name",
            "environments",
            "dynamicTools",
            "dynamic_tools",
            "selectedCapabilityRoots",
            "selected_capability_roots",
            "ephemeral",
            "history",
            "path",
            "experimentalRawEvents",
            "experimental_raw_events",
        ],
    )
}

fn reject_unmanaged_turn_overrides(request: &Value) -> Result<(), UpstreamError> {
    reject_unmanaged_params(
        request,
        &[
            "clientUserMessageId",
            "client_user_message_id",
            "turnTrigger",
            "turn_trigger",
            "toolOutput",
            "tool_output",
            "model",
            "serviceTier",
            "service_tier",
            "serviceTierForTurn",
            "service_tier_for_turn",
            "effort",
            "summary",
            "personality",
            "outputSchema",
            "output_schema",
            "cwd",
            "runtimeWorkspaceRoots",
            "runtime_workspace_roots",
            "approvalPolicy",
            "approval_policy",
            "approvalsReviewer",
            "approvals_reviewer",
            "sandboxPolicy",
            "sandbox_policy",
            "permissions",
            "environments",
            "collaborationMode",
            "collaboration_mode",
            "dynamicTools",
            "dynamic_tools",
            "selectedCapabilityRoots",
            "selected_capability_roots",
            "responsesapiClientMetadata",
            "responsesapi_client_metadata",
            "additionalContext",
            "additional_context",
            "cyberAccessProgram",
            "cyber_access_program",
        ],
    )
}

fn reject_unmanaged_params(request: &Value, forbidden: &[&str]) -> Result<(), UpstreamError> {
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .ok_or_else(|| UpstreamError::InvalidRequest("request has no parameters".into()))?;
    if let Some(field) = forbidden.iter().find(|field| params.contains_key(**field)) {
        return Err(UpstreamError::InvalidRequest(format!(
            "request may not override application-managed field: {field}"
        )));
    }
    Ok(())
}

fn thread_id_from_params(value: &Value) -> Result<String, UpstreamError> {
    value
        .pointer("/params/threadId")
        .or_else(|| value.pointer("/params/thread_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidRequest("request has no thread id".into()))
}

fn thread_id_from_response(value: &Value) -> Result<String, UpstreamError> {
    value
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidResponse("response has no thread id".into()))
}

pub(crate) fn thread_id_from_response_for_bridge(value: &Value) -> Result<String, UpstreamError> {
    thread_id_from_response(value)
}

fn turn_id_from_response(value: &Value) -> Result<&str, UpstreamError> {
    value
        .pointer("/turn/id")
        .or_else(|| value.pointer("/turnId"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| UpstreamError::InvalidResponse("turn response has no turn id".into()))
}

fn turn_id_from_params(value: &Value) -> Result<String, UpstreamError> {
    value
        .pointer("/params/turnId")
        .or_else(|| value.pointer("/params/turn_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidRequest("request has no turn id".into()))
}

fn active_turn_id_from_params(method: &str, value: &Value) -> Result<String, UpstreamError> {
    if method == "turn/steer" {
        value
            .pointer("/params/expectedTurnId")
            .or_else(|| value.pointer("/params/expected_turn_id"))
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                UpstreamError::InvalidRequest("turn/steer has no expected turn id".into())
            })
    } else {
        turn_id_from_params(value)
    }
}

fn ensure_expected_turn_id(value: &Value, expected: &str) -> Result<(), UpstreamError> {
    let actual = active_turn_id_from_params("turn/steer", value)?;
    if actual == expected {
        Ok(())
    } else {
        Err(UpstreamError::InvalidRequest(
            "turn/steer expected turn id does not match the active binding".into(),
        ))
    }
}

fn ensure_turn_id(value: &Value, expected: &str) -> Result<(), UpstreamError> {
    let actual = turn_id_from_params(value)?;
    if actual == expected {
        Ok(())
    } else {
        Err(UpstreamError::InvalidRequest(
            "turn/interrupt turn id does not match the active binding".into(),
        ))
    }
}

fn server_request_descriptor(
    id: &Value,
    method: &str,
    params: &Value,
) -> Option<ServerRequestDescriptor> {
    let request_id = serde_json::to_string(id).ok()?;
    let thread_id = params
        .get("threadId")
        .or_else(|| params.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let turn_id = params
        .get("turnId")
        .or_else(|| params.get("turn_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(ServerRequestDescriptor {
        request_id,
        method: method.to_string(),
        thread_id,
        turn_id,
    })
}

fn event_from_json(value: Value) -> Option<UpstreamEvent> {
    if value.get("id").is_some() {
        return None;
    }
    let method = value.get("method")?.as_str()?.to_string();
    let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
    Some(UpstreamEvent::ServerNotification { method, params })
}

fn io_error(error: io::Error) -> UpstreamError {
    UpstreamError::Io(error.to_string())
}
