//! Native side requests never acquire the prompt lock or change session bindings.
use std::path::PathBuf;
use std::time::Duration;

use sacp::{Agent, ConnectionTo, UntypedMessage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::agent_storage::AgentStoragePaths;
use super::error::AcpError;

pub const METHOD: &str = "_iyw/side_question";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideAction {
    Capabilities,
    Ask,
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SideQuestionRequest {
    pub session_id: String,
    pub action: SideAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
}

impl SideQuestionRequest {
    pub fn validate(&self) -> Result<(), AcpError> {
        if self.session_id.trim().is_empty() || self.session_id.len() > 256 {
            return Err(AcpError::protocol("Invalid side-question session"));
        }
        if self.action != SideAction::Capabilities
            && !self.request_id.as_ref().is_some_and(|id| {
                !id.is_empty()
                    && id.len() <= 100
                    && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(AcpError::protocol("Invalid side-question request id"));
        }
        if self.action == SideAction::Ask
            && !self
                .question
                .as_ref()
                .is_some_and(|q| !q.trim().is_empty() && q.len() <= 32_768)
        {
            return Err(AcpError::protocol(
                "Side question must contain 1–32768 bytes",
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct SideQuestionLane {
    closed: CancellationToken,
}

impl Drop for SideQuestionLane {
    fn drop(&mut self) {
        self.closed.cancel();
    }
}

impl SideQuestionLane {
    pub fn dispatch(
        &self,
        connection: ConnectionTo<Agent>,
        current_session: &str,
        request: SideQuestionRequest,
        reply: oneshot::Sender<Result<Value, AcpError>>,
    ) {
        if request.session_id != current_session {
            let _ = reply.send(Err(AcpError::protocol("Side-question session changed")));
            return;
        }
        let closed = self.closed.clone();
        tokio::spawn(async move {
            let ask = request.action == SideAction::Ask;
            let timeout = if ask { 180 } else { 15 };
            tracing::debug!(
                action = ?request.action,
                request_id = request.request_id.as_deref(),
                "[side-question] native request dispatched"
            );
            let result = tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(timeout), send(&connection, &request)) => {
                    match result {
                        Ok(result) => result,
                        Err(_) => {
                            tracing::warn!(
                                request_id = request.request_id.as_deref(),
                                "[side-question] native response deadline exceeded"
                            );
                            Err(AcpError::protocol("Native side-question request timed out"))
                        },
                    }
                }
                _ = closed.cancelled() => Err(AcpError::protocol("Side-question session closed")),
            };
            if ask && result.is_err() {
                let cancel = SideQuestionRequest {
                    action: SideAction::Cancel,
                    question: None,
                    ..request.clone()
                };
                let cleanup =
                    tokio::time::timeout(Duration::from_secs(10), send(&connection, &cancel)).await;
                tracing::warn!(
                    request_id = request.request_id.as_deref(),
                    cleanup_acknowledged = matches!(cleanup, Ok(Ok(_))),
                    "[side-question] request failed; cancellation attempted"
                );
            }
            let _ = reply.send(result);
        });
    }
}

async fn send(
    connection: &ConnectionTo<Agent>,
    request: &SideQuestionRequest,
) -> Result<Value, AcpError> {
    let message =
        UntypedMessage::new(METHOD, request).map_err(|e| AcpError::protocol(e.to_string()))?;
    let response: Result<Value, _> = connection
        .send_request_to(Agent, message)
        .block_task()
        .await;
    match response {
        Ok(value) if value.is_object() => Ok(value),
        Ok(_) => Err(AcpError::protocol("Invalid native side-question response")),
        Err(error)
            if request.action == SideAction::Capabilities && i32::from(error.code) == -32601 =>
        {
            Ok(json!({"supported": false}))
        }
        Err(error) => {
            // Native messages can echo user content; preserve the full error for
            // the caller, but do not put its payload in application logs.
            tracing::warn!(
                request_id = request.request_id.as_deref(),
                action = ?request.action,
                native_code = i32::from(error.code),
                "[side-question] native RPC rejected"
            );
            Err(AcpError::protocol(format!(
                "Native side-question request failed: {error}"
            )))
        }
    }
}

/// Content-addressed application glue, outside immutable third-party packages.
pub(crate) fn claude_bootstrap(storage: &AgentStoragePaths) -> Result<PathBuf, AcpError> {
    const SCRIPT: &str = include_str!("side_question/claude-bootstrap.mjs");
    let hash = format!("{:x}", Sha256::digest(SCRIPT.as_bytes()));
    let path = storage
        .runtime_dir()
        .join("app-bridges")
        .join(format!("side-{hash}.mjs"));
    super::builtin_prompt_bridge_file::ensure_plain_path(&path)?;
    let old = super::provider_overlay_files::read_optional(&path)
        .map_err(AcpError::BuiltinPromptInjection)?;
    super::provider_overlay_files::write_if_changed(&path, &old, SCRIPT)
        .map_err(AcpError::BuiltinPromptInjection)?;
    Ok(path)
}
