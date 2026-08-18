use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use rmcp::transport::streamable_http_server::{SessionId, SessionManager};

use super::super::binding::{BindProvisionalResult, ProvisionalBinding, SessionBindings};
use super::access::{text_response, unauthorized};
use super::{AuthHttpState, IssuedSessionContext, RequestSession, SESSION_HEADER};

pub(super) async fn bind_issued_session(
    state: &AuthHttpState,
    issued_id: Option<String>,
    issue: IssuedSessionContext,
) -> Result<(), Response> {
    let Some(issued_id) = issued_id else {
        return Ok(());
    };
    let IssuedSessionContext {
        principal,
        parent_connection_id,
        cancellation,
        delivery,
    } = issue;
    let bound = state
        .bindings
        .bind_provisional(
            issued_id.clone(),
            principal,
            parent_connection_id,
            &cancellation,
        )
        .await;
    match bound {
        BindProvisionalResult::Bound { ticket, replaced } => {
            close_replaced_session(state, replaced).await;
            if cancellation.is_cancelled() {
                rollback_session(state, ticket).await;
                return Err(unauthorized());
            }
            register_delivery_callbacks(state, &delivery, ticket);
            Ok(())
        }
        BindProvisionalResult::Cancelled => {
            close_session(state, &issued_id, "cancelled initialize").await;
            Err(unauthorized())
        }
        BindProvisionalResult::Conflict => {
            close_session(state, &issued_id, "binding conflict").await;
            Err(text_response(
                StatusCode::CONFLICT,
                "MCP session binding conflict",
            ))
        }
    }
}

pub(super) async fn cleanup_binding(
    state: &AuthHttpState,
    status: StatusCode,
    request: &RequestSession,
) {
    let Some(session_id) = request.session_id.as_deref() else {
        return;
    };
    if request.method == Method::DELETE && status.is_success() {
        state.bindings.remove(session_id).await;
    } else if status == StatusCode::NOT_FOUND {
        state
            .bindings
            .remove_authorized(session_id, request.principal)
            .await;
    }
}

pub(super) fn issued_session_id(response: &Response) -> Option<String> {
    response
        .headers()
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn register_delivery_callbacks(
    state: &AuthHttpState,
    delivery: &super::super::delivery::RelayDelivery,
    ticket: ProvisionalBinding,
) {
    let delivered = ticket.clone();
    let bindings = Arc::clone(&state.bindings);
    let protocol_sessions = Arc::clone(&state.protocol_sessions);
    delivery.register(
        Box::new(move || SessionBindings::mark_delivered(&delivered)),
        Box::new(move || {
            if !SessionBindings::mark_aborted(&ticket) {
                return;
            }
            tokio::spawn(async move {
                if !bindings.remove_ticket(&ticket).await {
                    return;
                }
                close_with_manager(
                    &protocol_sessions,
                    ticket.session_id(),
                    "undelivered initialize",
                )
                .await;
            });
        }),
    );
}

async fn rollback_session(state: &AuthHttpState, ticket: ProvisionalBinding) {
    if SessionBindings::mark_aborted(&ticket) && state.bindings.remove_ticket(&ticket).await {
        close_session(state, ticket.session_id(), "cancelled initialize").await;
    }
}

async fn close_replaced_session(state: &AuthHttpState, session_id: Option<SessionId>) {
    let Some(session_id) = session_id else {
        return;
    };
    close_with_manager(
        &state.protocol_sessions,
        session_id.as_ref(),
        "replaced initialize",
    )
    .await;
}

async fn close_session(state: &AuthHttpState, session_id: &str, reason: &'static str) {
    close_with_manager(&state.protocol_sessions, session_id, reason).await;
}

async fn close_with_manager(
    sessions: &rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
    session_id: &str,
    reason: &'static str,
) {
    let session: SessionId = Arc::<str>::from(session_id);
    if let Err(error) = sessions.close_session(&session).await {
        tracing::warn!(
            target: "builtin_mcp",
            reason,
            error = %error,
            "failed to close HTTP MCP protocol session"
        );
    }
}
