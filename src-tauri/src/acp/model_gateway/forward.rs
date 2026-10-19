use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::{Request, State, WebSocketUpgrade},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{stream, StreamExt};

use super::{
    error::GatewayError,
    headers,
    limits::HEADER_TIMEOUT,
    rewrite::{self, Output},
    spool::Spool,
    target, Runtime,
};

pub(super) async fn handle(
    State(runtime): State<Arc<Runtime>>,
    socket: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
    request: Request,
) -> Response {
    let stop = runtime.shutdown.clone();
    let result = tokio::select! {
        _ = stop.cancelled() => Err(GatewayError::unavailable("Local model gateway is stopping")),
        result = dispatch(runtime, socket.ok(), request) => result,
    };
    match result {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn dispatch(
    runtime: Arc<Runtime>,
    socket: Option<WebSocketUpgrade>,
    request: Request,
) -> Result<Response, GatewayError> {
    let (parts, body) = request.into_parts();
    authenticate(&runtime, &parts.headers)?;
    let path = parts
        .uri
        .path()
        .strip_prefix(&runtime.prefix)
        .ok_or_else(|| GatewayError::input("Invalid local model endpoint"))?
        .to_owned();
    let mut target = reqwest::Url::parse(&target::resolve(&runtime.upstream, &path)?)
        .map_err(|_| GatewayError::input("Invalid model gateway target"))?;
    target.set_query(parts.uri.query());
    let token = headers::token(&parts.headers)?;
    super::auth::check(&token)?;
    let permit = runtime
        .requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| GatewayError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "local_gateway_busy",
            message: "Local model gateway is at capacity",
        })?;
    if let Some(socket) = socket {
        return super::websocket::upgrade(
            runtime,
            (socket, parts.headers, target.to_string()),
            (path, token, permit),
        )
        .await;
    }
    if !matches!(parts.method, Method::GET | Method::POST) {
        return Err(GatewayError::input("Unsupported model HTTP method"));
    }
    forward_http(runtime, (parts, body), (path, token, target, permit)).await
}

fn authenticate(runtime: &Runtime, headers: &axum::http::HeaderMap) -> Result<(), GatewayError> {
    let host = headers.get("host").and_then(|value| value.to_str().ok());
    if host != Some(runtime.authority.as_str()) || headers.contains_key("origin") {
        return Err(GatewayError {
            status: StatusCode::FORBIDDEN,
            code: "local_gateway_forbidden",
            message: "Local gateway authority is invalid",
        });
    }
    Ok(())
}

async fn forward_http(
    runtime: Arc<Runtime>,
    (parts, body): (axum::http::request::Parts, Body),
    (path, token, target, permit): (
        String,
        String,
        reqwest::Url,
        tokio::sync::OwnedSemaphorePermit,
    ),
) -> Result<Response, GatewayError> {
    let started = Instant::now();
    let body = tokio::time::timeout(super::limits::PREPARE_TIMEOUT, Spool::receive(body))
        .await
        .map_err(|_| GatewayError::input("Local request body timed out"))??;
    let original_bytes = body.len();
    let prepared = if rewritable(&parts.headers) && original_bytes > 0 {
        rewrite::prepare(&runtime, body, (&path, &token)).await?
    } else {
        Output::Request(body)
    };
    super::auth::check(&token)?;
    let body = match prepared {
        Output::Request(body) => body,
        Output::Prepared(asset) => return Ok(axum::Json(asset).into_response()),
    };
    let final_bytes = body.len();
    let request = runtime
        .client
        .request(parts.method, target)
        .headers(headers::filtered(&parts.headers))
        .header("token", &token)
        .header("x-iyw-native-vision", "true")
        .header("accept-encoding", "identity")
        .header("content-length", final_bytes)
        .body(body.into_body()?);
    let response = tokio::time::timeout(HEADER_TIMEOUT, request.send())
        .await
        .map_err(|_| GatewayError::unavailable("Cloud model response headers timed out"))?
        .map_err(|_| GatewayError::unavailable("Cloud model gateway could not be reached"))?;
    tracing::debug!(target: "model_gateway", original_bytes, final_bytes,
        elapsed_ms = started.elapsed().as_millis(), status = response.status().as_u16(), "local model request forwarded");
    Ok(stream_response(response, permit, runtime.shutdown.clone()))
}

fn rewritable(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
        })
        && !headers
            .get("content-encoding")
            .is_some_and(|value| value != "identity")
}

fn stream_response(
    response: reqwest::Response,
    permit: tokio::sync::OwnedSemaphorePermit,
    stop: tokio_util::sync::CancellationToken,
) -> Response {
    let status = response.status();
    let response_headers = headers::filtered(response.headers());
    let stream = response.bytes_stream();
    let guarded = stream::unfold(
        (Box::pin(stream), permit, stop),
        |(mut input, permit, stop)| async move {
            let next = tokio::select! { _ = stop.cancelled() => None, next = input.next() => next };
            next.map(|chunk| {
                (
                    chunk.map_err(|_| std::io::Error::other("Cloud model stream interrupted")),
                    (input, permit, stop),
                )
            })
        },
    );
    let mut output = Response::new(Body::from_stream(guarded));
    *output.status_mut() = status;
    *output.headers_mut() = response_headers;
    output
}
