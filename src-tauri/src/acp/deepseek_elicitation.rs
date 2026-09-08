use std::sync::Arc;
use std::time::Duration;

use sacp::schema::{ElicitationAction, SessionId};
use sacp::{JsonRpcRequest, Responder};
use serde_json::Value;

use crate::acp::deepseek_elicitation_form::{decline_response, parse_request, FormPlan};
use crate::acp::question::{QuestionRuntimeConfig, SessionQuestionAccess};
use crate::acp::runtime_host_router::SessionRequestRouter;
use crate::acp::session_state::SessionState;
use crate::acp::types::AcpEvent;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

#[path = "deepseek_elicitation_lifecycle.rs"]
mod lifecycle;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonRpcRequest)]
#[request(method = "elicitation/create", response = serde_json::Value)]
#[serde(transparent)]
pub(crate) struct ElicitationCreateRequest(pub(crate) Value);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sacp::JsonRpcNotification)]
#[notification(method = "_iyw/elicitation/cancel")]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElicitationCancelNotification {
    session_id: SessionId,
    request_key: String,
}

const QUESTION_STATE_INTERVAL: Duration = Duration::from_millis(250);

struct QuestionLifecycle {
    deadline: Option<tokio::time::Instant>,
    questions: Arc<dyn SessionQuestionAccess>,
    turn_generation: i64,
    track_turn: bool,
    agent: crate::models::agent::AgentType,
    request: Option<lifecycle::ElicitationLease>,
}

pub(crate) struct ElicitationAccess {
    questions: Arc<dyn SessionQuestionAccess>,
    config: QuestionRuntimeConfig,
    pending: lifecycle::ElicitationRegistry,
}

impl ElicitationAccess {
    pub(crate) fn new(
        questions: Arc<dyn SessionQuestionAccess>,
        config: QuestionRuntimeConfig,
    ) -> Self {
        Self { questions, config, pending: Default::default() }
    }
}

impl SessionRequestRouter {
    pub(super) async fn cancel_elicitation(&self, notification: ElicitationCancelNotification) {
        if let Some(route) = self.resolve(&notification.session_id) {
            if notification.request_key.starts_with("session/request_permission:") {
                crate::acp::permission_runtime::PermissionRuntime::new(&route.state, &route.emitter, &route.permissions)
                    .cancel_key(&notification.request_key).await;
            } else if let Some(access) = &route.elicitation {
                access.pending.cancel(&notification.request_key);
            }
        }
    }

    pub(super) async fn elicitation(
        &self,
        request: ElicitationCreateRequest,
        responder: Responder<Value>,
    ) -> Result<(), sacp::Error> {
        let request_key = request.0.pointer("/_meta/iyw/requestKey").and_then(Value::as_str).map(str::to_string);
        let deadline = request.0.pointer("/_meta/codex/autoResolutionMs").and_then(Value::as_u64)
            .and_then(|millis| tokio::time::Instant::now().checked_add(Duration::from_millis(millis)));
        let (session_id, plan) = match parse_request(request.0) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(
                    error,
                    "[ACP] elicitation request declined"
                );
                respond_decline(responder);
                return Ok(());
            }
        };
        let Some(route) = self.resolve(&session_id) else {
            tracing::warn!(
                session_id = %session_id,
                "[ACP] elicitation route is unavailable"
            );
            respond_decline(responder);
            return Ok(());
        };
        let Some(access) = route.elicitation.as_ref() else {
            tracing::warn!(
                session_id = %session_id,
                "[ACP] elicitation interaction access is unavailable"
            );
            respond_decline(responder);
            return Ok(());
        };
        let request_lease = match request_key {
            Some(key) => match access.pending.register(&key) {
                Some(lease) => Some(lease),
                None => { respond_decline(responder); return Ok(()); }
            },
            None => None,
        };
        if !plan.is_approval() && !access.config.is_enabled().await {
            tracing::info!(
                session_id = %session_id,
                "[ACP] elicitation declined because ask-user is disabled"
            );
            respond_decline(responder);
            return Ok(());
        }
        let snapshot = route.state.read().await;
        let connection_id = snapshot.connection_id.clone();
        let lifecycle = QuestionLifecycle {
            deadline,
            questions: Arc::clone(&access.questions),
            turn_generation: snapshot.turn_generation,
            track_turn: snapshot.agent_type == crate::models::agent::AgentType::Codex && snapshot.turn_in_flight,
            agent: snapshot.agent_type,
            request: request_lease,
        };
        drop(snapshot);
        let state = Arc::clone(&route.state);
        let emitter = route.emitter.clone();
        let Some(registered) = access
            .questions
            .register_question(&connection_id, plan.specs().iter().take(crate::acp::question::MAX_QUESTIONS).cloned().collect())
            .await
        else {
            tracing::warn!(
                agent = %lifecycle.agent,
                connection_id,
                session_id = %session_id,
                "[ACP] elicitation could not be registered"
            );
            respond_decline(responder);
            return Ok(());
        };
        tracing::info!(
            agent = %lifecycle.agent,
            connection_id,
            session_id = %session_id,
            question_id = registered.question_id,
            field_count = plan.field_count(),
            "[ACP] elicitation waiting for user input"
        );
        spawn_response_task(
            session_id,
            connection_id,
            state,
            emitter,
            plan,
            registered,
            responder,
            lifecycle,
        );
        Ok(())
    }
}

fn spawn_response_task(
    session_id: SessionId,
    connection_id: String,
    state: Arc<tokio::sync::RwLock<SessionState>>,
    emitter: EventEmitter,
    plan: FormPlan,
    mut registered: crate::acp::question::RegisteredQuestion,
    responder: Responder<Value>,
    mut lifecycle: QuestionLifecycle,
) {
    tokio::spawn(async move {
        let outcome = lifecycle::wait_for_form(&plan, &mut registered, (&state, &connection_id, &mut lifecycle)).await;
        let response = match outcome {
            Some(outcome) => {
                if let Some(tool_call_id) = plan.tool_call_id() {
                    emit_with_state(
                        &state,
                        &emitter,
                        AcpEvent::ToolCall {
                            tool_call_id: tool_call_id.to_string(),
                            title: "request_user_input".to_string(),
                            kind: "other".to_string(),
                            status: "completed".to_string(),
                            content: None,
                            raw_input: Some(plan.result_card_input().to_string()),
                            raw_output: Some(plan.result_card_output(&outcome).to_string()),
                            locations: None,
                            meta: None,
                            images: None,
                        },
                    )
                    .await;
                }
                plan.response(&outcome)
            }
            None => decline_response(),
        };
        let action = match &response.action {
            ElicitationAction::Accept(_) => "accept",
            ElicitationAction::Decline => "decline",
            ElicitationAction::Cancel => "cancel",
            _ => "unknown",
        };
        tracing::info!(
            agent = %lifecycle.agent,
            connection_id,
            session_id = %session_id,
            action,
            "[ACP] elicitation resolved"
        );
        let _ = responder.respond(serde_json::to_value(response).unwrap_or_default());
    });
}

fn respond_decline(responder: Responder<Value>) {
    let _ = responder.respond(serde_json::to_value(decline_response()).unwrap_or_default());
}
