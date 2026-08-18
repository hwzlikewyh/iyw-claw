use std::sync::Arc;

use sacp::schema::{ElicitationAction, SessionId};
use sacp::{JsonRpcRequest, Responder};
use serde_json::Value;

use crate::acp::deepseek_elicitation_form::{decline_response, parse_request, FormPlan};
use crate::acp::question::{QuestionRuntimeConfig, SessionQuestionAccess};
use crate::acp::runtime_host_router::SessionRequestRouter;
use crate::acp::session_state::SessionState;
use crate::acp::types::AcpEvent;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, JsonRpcRequest)]
#[request(method = "elicitation/create", response = serde_json::Value)]
#[serde(transparent)]
pub(crate) struct ElicitationCreateRequest(pub(crate) Value);

pub(crate) struct ElicitationAccess {
    questions: Arc<dyn SessionQuestionAccess>,
    config: QuestionRuntimeConfig,
}

impl ElicitationAccess {
    pub(crate) fn new(
        questions: Arc<dyn SessionQuestionAccess>,
        config: QuestionRuntimeConfig,
    ) -> Self {
        Self { questions, config }
    }
}

impl SessionRequestRouter {
    pub(super) async fn elicitation(
        &self,
        request: ElicitationCreateRequest,
        responder: Responder<Value>,
    ) -> Result<(), sacp::Error> {
        let (session_id, plan) = match parse_request(request.0) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(
                    agent = "deepseek",
                    error,
                    "[ACP] elicitation request declined"
                );
                respond_decline(responder);
                return Ok(());
            }
        };
        let Some(route) = self.resolve(&session_id) else {
            tracing::warn!(
                agent = "deepseek",
                session_id = %session_id,
                "[ACP] elicitation route is unavailable"
            );
            respond_decline(responder);
            return Ok(());
        };
        let Some(access) = route.elicitation.as_ref() else {
            tracing::warn!(
                agent = "deepseek",
                session_id = %session_id,
                "[ACP] elicitation interaction access is unavailable"
            );
            respond_decline(responder);
            return Ok(());
        };
        if !plan.is_approval() && !access.config.is_enabled().await {
            tracing::info!(
                agent = "deepseek",
                session_id = %session_id,
                "[ACP] elicitation declined because ask-user is disabled"
            );
            respond_decline(responder);
            return Ok(());
        }
        let connection_id = route.state.read().await.connection_id.clone();
        let state = Arc::clone(&route.state);
        let emitter = route.emitter.clone();
        let Some(registered) = access
            .questions
            .register_question(&connection_id, plan.specs().to_vec())
            .await
        else {
            tracing::warn!(
                agent = "deepseek",
                connection_id,
                session_id = %session_id,
                "[ACP] elicitation could not be registered"
            );
            respond_decline(responder);
            return Ok(());
        };
        tracing::info!(
            agent = "deepseek",
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
    registered: crate::acp::question::RegisteredQuestion,
    responder: Responder<Value>,
) {
    tokio::spawn(async move {
        let response = match registered.answer_rx.await {
            Ok(outcome) => {
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
            Err(_) => decline_response(),
        };
        let action = match &response.action {
            ElicitationAction::Accept(_) => "accept",
            ElicitationAction::Decline => "decline",
            ElicitationAction::Cancel => "cancel",
            _ => "unknown",
        };
        tracing::info!(
            agent = "deepseek",
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
