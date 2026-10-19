use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{ws::Message, WebSocketUpgrade},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::sync::OwnedSemaphorePermit;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{client::IntoClientRequest, protocol::Message as UpstreamMessage},
};

use super::{
    error::GatewayError,
    headers,
    limits::MAX_WEBSOCKET_BYTES,
    rewrite::{self, Output},
    spool::Spool,
    Runtime,
};

pub(super) async fn upgrade(
    runtime: Arc<Runtime>,
    (socket, headers, target): (WebSocketUpgrade, HeaderMap, String),
    (path, token, permit): (String, String, OwnedSemaphorePermit),
) -> Result<Response, GatewayError> {
    let socket_permit = runtime
        .sockets
        .clone()
        .try_acquire_owned()
        .map_err(|_| GatewayError::unavailable("Local model WebSocket capacity reached"))?;
    let request = handshake(&target, &headers, &token)?;
    let result = open_cloud(request).await;
    let (upstream, response) = match result {
        Ok(result) => result,
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            let (parts, body) = response.into_parts();
            let mut output = Response::new(axum::body::Body::from(body.unwrap_or_default()));
            *output.status_mut() = parts.status;
            *output.headers_mut() = headers::filtered(&parts.headers);
            return Ok(output);
        }
        Err(_) => {
            return Err(GatewayError::unavailable(
                "Cloud model WebSocket is unavailable",
            ))
        }
    };
    let socket = match response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
    {
        Some(protocol) => socket.protocols([protocol.to_owned()]),
        None => socket,
    };
    Ok(socket
        .max_message_size(MAX_WEBSOCKET_BYTES)
        .max_frame_size(MAX_WEBSOCKET_BYTES)
        .on_upgrade(move |client| {
            bridge(
                runtime,
                (client, upstream),
                (path, token, (permit, socket_permit)),
            )
        })
        .into_response())
}

fn handshake(
    target: &str,
    headers: &HeaderMap,
    token: &str,
) -> Result<axum::http::Request<()>, GatewayError> {
    let target = target
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut request = target
        .into_client_request()
        .map_err(|_| GatewayError::input("Invalid model WebSocket target"))?;
    for (name, value) in headers::filtered(&headers) {
        if let Some(name) = name {
            if !name.as_str().starts_with("sec-websocket-") {
                request.headers_mut().insert(name, value);
            }
        }
    }
    request.headers_mut().insert(
        "token",
        token
            .parse()
            .map_err(|_| GatewayError::input("Invalid model authentication"))?,
    );
    if let Some(protocols) = headers.get("sec-websocket-protocol") {
        request
            .headers_mut()
            .insert("sec-websocket-protocol", protocols.clone());
    }
    request.headers_mut().insert(
        "x-iyw-native-vision",
        axum::http::HeaderValue::from_static("true"),
    );
    Ok(request)
}

async fn open_cloud(
    request: axum::http::Request<()>,
) -> Result<(Remote, axum::http::Response<Option<Vec<u8>>>), tokio_tungstenite::tungstenite::Error>
{
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_WEBSOCKET_BYTES))
        .max_frame_size(Some(MAX_WEBSOCKET_BYTES));
    tokio::time::timeout(
        super::limits::CONNECT_TIMEOUT,
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| {
        tokio_tungstenite::tungstenite::Error::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Model socket timed out",
        ))
    })?
}

type Remote =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn bridge(
    runtime: Arc<Runtime>,
    (client, upstream): (axum::extract::ws::WebSocket, Remote),
    (path, token, permit): (String, String, (OwnedSemaphorePermit, OwnedSemaphorePermit)),
) {
    let _permit = permit;
    let (remote_send, remote_read) = upstream.split();
    let (client_send, client_read) = client.split();
    tokio::select! {
        _ = runtime.shutdown.cancelled() => {},
        result = client_to_cloud(&runtime, (client_read, remote_send), (&path, &token)) => {
            if let Err(error) = result {
                tracing::warn!(target: "model_gateway", code = error.code, stage = "websocket_prepare", "local image preparation failed; socket closed");
            }
        },
        _ = cloud_to_client(remote_read, client_send) => {},
    }
}

async fn client_to_cloud(
    runtime: &Arc<Runtime>,
    (mut client, mut remote): (
        SplitStream<axum::extract::ws::WebSocket>,
        SplitSink<Remote, UpstreamMessage>,
    ),
    context: (&str, &str),
) -> Result<(), GatewayError> {
    while let Some(Ok(message)) = client.next().await {
        super::auth::check(context.1)?;
        let closing = matches!(message, Message::Close(_));
        let mapped = match message {
            Message::Text(text) => UpstreamMessage::Text(
                prepare_text(runtime, text.as_bytes(), context)
                    .await?
                    .into(),
            ),
            Message::Binary(data) => UpstreamMessage::Binary(data),
            Message::Ping(data) => UpstreamMessage::Ping(data),
            Message::Pong(data) => UpstreamMessage::Pong(data),
            Message::Close(frame) => UpstreamMessage::Close(frame.map(|frame| {
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: frame.code.into(),
                    reason: frame.reason.as_str().into(),
                }
            })),
        };
        if remote.send(mapped).await.is_err() || closing {
            break;
        }
    }
    Ok(())
}

async fn cloud_to_client(
    mut remote: SplitStream<Remote>,
    mut client: SplitSink<axum::extract::ws::WebSocket, Message>,
) {
    while let Some(Ok(message)) = remote.next().await {
        let closing = matches!(message, UpstreamMessage::Close(_));
        let mapped = match message {
            UpstreamMessage::Text(text) => Message::Text(text.as_str().into()),
            UpstreamMessage::Binary(data) => Message::Binary(data),
            UpstreamMessage::Ping(data) => Message::Ping(data),
            UpstreamMessage::Pong(data) => Message::Pong(data),
            UpstreamMessage::Close(frame) => {
                Message::Close(frame.map(|frame| axum::extract::ws::CloseFrame {
                    code: frame.code.into(),
                    reason: frame.reason.as_str().into(),
                }))
            }
            UpstreamMessage::Frame(_) => continue,
        };
        if client.send(mapped).await.is_err() || closing {
            break;
        }
    }
}

async fn prepare_text(
    runtime: &Arc<Runtime>,
    bytes: &[u8],
    context: (&str, &str),
) -> Result<String, GatewayError> {
    let body = Spool::Memory(Bytes::copy_from_slice(bytes));
    let output = rewrite::prepare(runtime, body, context).await?;
    super::auth::check(context.1)?;
    let Output::Request(body) = output else {
        return Err(GatewayError::input("Invalid model WebSocket request"));
    };
    super::work::blocking(runtime, move || {
        let mut text = String::new();
        body.reader()?.read_to_string(&mut text)?;
        Ok(text)
    })
    .await
}

use std::io::Read;
