use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod access;
mod session_binding;

use access::{
    authenticate_access, bounded_request, request_metadata, text_response, AuthenticatedAccess,
};
use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use tokio::sync::Semaphore;

use super::authority::SessionContext;
use super::binding::{Principal, SessionBindings};
use super::delivery::{wrap_delivery, RelayDelivery};
use super::session::SessionRegistry;
use session_binding::{bind_issued_session, cleanup_binding, issued_session_id};

const MAX_ACTIVE_REQUESTS: usize = 128;
const MAX_ACTIVE_STREAMS: usize = 128;
const SESSION_HEADER: &str = "mcp-session-id";

#[derive(Clone)]
pub(super) struct AuthenticatedRequest {
    context: SessionContext,
}

impl AuthenticatedRequest {
    pub(super) fn context(&self) -> &SessionContext {
        &self.context
    }
}

#[derive(Clone)]
pub(super) struct AuthHttpState {
    authority: Arc<str>,
    allowed_origin: Arc<str>,
    registry: Arc<SessionRegistry>,
    bindings: Arc<SessionBindings>,
    protocol_sessions: Arc<LocalSessionManager>,
    ready: Arc<AtomicBool>,
    concurrency: Arc<Semaphore>,
    stream_concurrency: Arc<Semaphore>,
}

struct PendingForward {
    request: Request<Body>,
    next: Next,
    access: AuthenticatedAccess,
}

struct RequestSession {
    method: Method,
    principal: Principal,
    session_id: Option<String>,
}

struct IssuedSessionContext {
    principal: Principal,
    parent_connection_id: String,
    cancellation: tokio_util::sync::CancellationToken,
    delivery: RelayDelivery,
}

impl AuthHttpState {
    pub(super) fn new(
        authority: String,
        registry: Arc<SessionRegistry>,
        bindings: Arc<SessionBindings>,
        protocol_sessions: Arc<LocalSessionManager>,
        ready: Arc<AtomicBool>,
    ) -> Self {
        Self {
            allowed_origin: format!("http://{authority}").into(),
            authority: authority.into(),
            registry,
            bindings,
            protocol_sessions,
            ready,
            concurrency: Arc::new(Semaphore::new(MAX_ACTIVE_REQUESTS)),
            stream_concurrency: Arc::new(Semaphore::new(MAX_ACTIVE_STREAMS)),
        }
    }
}

pub(super) async fn authenticate_request(
    State(state): State<AuthHttpState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.ready.load(Ordering::Acquire) {
        return text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "MCP service is shutting down",
        );
    }
    let metadata = match request_metadata(&state, &request) {
        Ok(metadata) => metadata,
        Err(response) => return response,
    };
    let access = match authenticate_access(&state, metadata).await {
        Ok(access) => access,
        Err(response) => return response,
    };
    forward_request(
        &state,
        PendingForward {
            request,
            next,
            access,
        },
    )
    .await
}

async fn forward_request(state: &AuthHttpState, pending: PendingForward) -> Response {
    let PendingForward {
        request,
        next,
        access,
    } = pending;
    let AuthenticatedAccess {
        context,
        principal,
        session_id,
        global_permit,
        session_permit,
    } = access;
    let request_session = RequestSession {
        method: request.method().clone(),
        principal,
        session_id,
    };
    let new_session = request_session.session_id.is_none();
    let mut request = match bounded_request(request, new_session).await {
        Ok(request) => request,
        Err(response) => return response,
    };
    let parent_connection_id = context.connection_id().to_string();
    let authority_cancellation = context.cancellation().clone();
    let delivery = RelayDelivery::default();
    request
        .extensions_mut()
        .insert(AuthenticatedRequest { context });
    request.extensions_mut().insert(delivery.clone());
    let mut downstream = next.run(request).await;
    if new_session {
        let issue = IssuedSessionContext {
            principal,
            parent_connection_id,
            cancellation: authority_cancellation,
            delivery: delivery.clone(),
        };
        if let Err(response) =
            bind_issued_session(state, issued_session_id(&downstream), issue).await
        {
            delivery.abort();
            return response;
        }
    }
    cleanup_binding(state, downstream.status(), &request_session).await;
    wrap_delivery(&mut downstream, delivery, (global_permit, session_permit));
    downstream
}
