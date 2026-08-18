use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, HOST, ORIGIN, WWW_AUTHENTICATE};
use axum::http::{Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::{SessionId, SessionManager};
use tokio::sync::OwnedSemaphorePermit;

use super::{AuthHttpState, SESSION_HEADER};
use crate::acp::builtin_mcp::authority::SessionContext;
use crate::acp::builtin_mcp::binding::Principal;

const MAX_BODY_BYTES: usize = 1024 * 1024;

pub(super) struct AuthenticatedAccess {
    pub(super) context: SessionContext,
    pub(super) principal: Principal,
    pub(super) session_id: Option<String>,
    pub(super) global_permit: OwnedSemaphorePermit,
    pub(super) session_permit: OwnedSemaphorePermit,
}

pub(super) struct RequestMetadata {
    method: Method,
    bearer: String,
    session_id: Option<String>,
}

pub(super) async fn authenticate_access(
    state: &AuthHttpState,
    metadata: RequestMetadata,
) -> Result<AuthenticatedAccess, Response> {
    let stream_request = metadata.method == Method::GET;
    let global_capacity = if stream_request {
        &state.stream_concurrency
    } else {
        &state.concurrency
    };
    let Ok(global_permit) = global_capacity.clone().try_acquire_owned() else {
        return Err(text_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "MCP request capacity reached",
        ));
    };
    let Some(context) = state.registry.lookup(&metadata.bearer).await else {
        return Err(unauthorized());
    };
    let session_permit = if stream_request {
        context.try_acquire_stream()
    } else {
        context.try_acquire_request()
    };
    let Some(session_permit) = session_permit else {
        return Err(text_response(
            StatusCode::TOO_MANY_REQUESTS,
            "MCP session request capacity reached",
        ));
    };
    let principal = Principal::from_bearer(&metadata.bearer);
    let session_id = validate_session(state, &metadata, principal).await?;
    Ok(AuthenticatedAccess {
        context,
        principal,
        session_id,
        global_permit,
        session_permit,
    })
}

async fn validate_session(
    state: &AuthHttpState,
    metadata: &RequestMetadata,
    principal: Principal,
) -> Result<Option<String>, Response> {
    if let Some(ref session_id) = metadata.session_id {
        if !state
            .bindings
            .confirm_and_authorize(session_id, principal)
            .await
        {
            return Err(text_response(
                StatusCode::NOT_FOUND,
                "MCP session not found",
            ));
        }
        let protocol_id: SessionId = Arc::<str>::from(session_id.as_str());
        if !state
            .protocol_sessions
            .has_session(&protocol_id)
            .await
            .unwrap_or(false)
        {
            state
                .bindings
                .remove_authorized(session_id, principal)
                .await;
            return Err(text_response(
                StatusCode::NOT_FOUND,
                "MCP session not found",
            ));
        }
    } else {
        prune_stale_sessions(state, principal).await;
    }
    Ok(metadata.session_id.clone())
}

pub(super) fn request_metadata(
    state: &AuthHttpState,
    request: &Request<Body>,
) -> Result<RequestMetadata, Response> {
    if !valid_origin_and_host(state, request) {
        return Err(text_response(StatusCode::FORBIDDEN, "Forbidden"));
    }
    let Some(bearer) = bearer_token(request).map(str::to_owned) else {
        return Err(unauthorized());
    };
    let session_id = unique_header_value(request, SESSION_HEADER)
        .map_err(|()| text_response(StatusCode::BAD_REQUEST, "Invalid MCP session id"))?
        .map(str::to_owned);
    let method = request.method().clone();
    if session_id.is_none() && method != Method::POST {
        return Err(text_response(
            StatusCode::BAD_REQUEST,
            "MCP session id is required",
        ));
    }
    Ok(RequestMetadata {
        method,
        bearer,
        session_id,
    })
}

fn valid_origin_and_host(state: &AuthHttpState, request: &Request<Body>) -> bool {
    let host_ok = unique_header_value(request, HOST.as_str())
        .is_ok_and(|host| host.is_some_and(|value| value.eq_ignore_ascii_case(&state.authority)));
    let origin_ok = unique_header_value(request, ORIGIN.as_str()).is_ok_and(|origin| {
        origin.is_none_or(|value| value.eq_ignore_ascii_case(&state.allowed_origin))
    });
    host_ok && origin_ok
}

fn bearer_token(request: &Request<Body>) -> Option<&str> {
    let mut values = request.headers().get_all(AUTHORIZATION).iter();
    let raw = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    let (scheme, token) = raw.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}

fn unique_header_value<'a>(request: &'a Request<Body>, name: &str) -> Result<Option<&'a str>, ()> {
    let mut values = request.headers().get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    value.to_str().map(Some).map_err(|_| ())
}

async fn prune_stale_sessions(state: &AuthHttpState, principal: Principal) {
    for session_id in state.bindings.principal_sessions(principal).await {
        let protocol_id: SessionId = Arc::<str>::from(session_id.as_str());
        let active = state
            .protocol_sessions
            .has_session(&protocol_id)
            .await
            .unwrap_or(false);
        if !active {
            state
                .bindings
                .remove_authorized(&session_id, principal)
                .await;
        }
    }
}

pub(super) async fn bounded_request(
    request: Request<Body>,
    require_initialize: bool,
) -> Result<Request<Body>, Response> {
    if request.method() != Method::POST {
        return Ok(request);
    }
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| text_response(StatusCode::PAYLOAD_TOO_LARGE, "MCP request body too large"))?;
    if require_initialize {
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
            text_response(StatusCode::BAD_REQUEST, "Invalid MCP initialize request")
        })?;
        let valid = value.get("method").and_then(|item| item.as_str()) == Some("initialize")
            && value.get("id").is_some_and(|id| !id.is_null());
        if !valid {
            return Err(text_response(
                StatusCode::BAD_REQUEST,
                "A new MCP session must start with initialize",
            ));
        }
    }
    Ok(Request::from_parts(parts, Body::from(bytes)))
}

pub(super) fn unauthorized() -> Response {
    let mut response = text_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, "Bearer".parse().expect("valid header"));
    response
}

pub(super) fn text_response(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
