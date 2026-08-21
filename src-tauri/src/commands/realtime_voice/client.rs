use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{http::Request, Message},
    MaybeTlsStream, WebSocketStream,
};

use crate::app_error::AppCommandError;
use crate::commands::skill_market::client as fusion_client;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_TEXT_EVENT_BYTES: usize = 64 * 1024;

pub(super) type VoiceSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum GatewayEvent {
    Ready {
        session_id: String,
    },
    Partial {
        sequence: u64,
        text: String,
        #[serde(default)]
        start_ms: Option<u64>,
        #[serde(default)]
        end_ms: Option<u64>,
    },
    Final {
        sequence: u64,
        text: String,
        #[serde(default)]
        start_ms: Option<u64>,
        #[serde(default)]
        end_ms: Option<u64>,
    },
    Completed {
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    Error {
        code: String,
        message: String,
    },
    #[serde(other)]
    Unknown,
}

pub(super) async fn connect(token: &str) -> Result<VoiceSocket, AppCommandError> {
    let url = websocket_url()?;
    let request = websocket_request(&url, token)?;
    let (mut socket, response) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| AppCommandError::network("Realtime voice connection timed out"))?
        .map_err(connection_error)?;
    tracing::info!(
        status = %response.status(),
        "[RealtimeVoice] Fusion WebSocket connected"
    );
    send_auth(&mut socket, token).await?;
    wait_for_ready(&mut socket).await?;
    Ok(socket)
}

fn websocket_request(url: &reqwest::Url, token: &str) -> Result<Request<()>, AppCommandError> {
    Request::builder()
        .uri(url.as_str())
        .header("token", token)
        .body(())
        .map_err(|error| {
            AppCommandError::configuration_invalid("Invalid realtime voice gateway request")
                .with_detail(error.to_string())
        })
}

fn connection_error(error: tokio_tungstenite::tungstenite::Error) -> AppCommandError {
    if let tokio_tungstenite::tungstenite::Error::Http(response) = &error {
        let status = response.status();
        let message = if status.is_success() || status.as_u16() == 400 {
            "Realtime voice gateway did not accept the WebSocket upgrade"
        } else {
            "Realtime voice gateway rejected the WebSocket connection"
        };
        return AppCommandError::network(message).with_detail(status.to_string());
    }
    AppCommandError::network("Realtime voice connection failed").with_detail(error.to_string())
}

pub(super) fn parse_event(message: Message) -> Result<Option<GatewayEvent>, String> {
    let Message::Text(text) = message else {
        return Ok(None);
    };
    if text.len() > MAX_TEXT_EVENT_BYTES {
        return Err("Realtime voice event exceeded the size limit".to_string());
    }
    serde_json::from_str::<GatewayEvent>(&text)
        .map(Some)
        .map_err(|error| format!("Invalid realtime voice event: {error}"))
}

fn websocket_url() -> Result<reqwest::Url, AppCommandError> {
    let mut url = fusion_client::endpoint("/v1/voice/realtime/connect")?;
    let scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        _ => {
            return Err(AppCommandError::configuration_invalid(
                "Invalid realtime voice gateway scheme",
            ))
        }
    };
    url.set_scheme(scheme).map_err(|_| {
        AppCommandError::configuration_invalid("Invalid realtime voice gateway URL")
    })?;
    Ok(url)
}

async fn send_auth(socket: &mut VoiceSocket, token: &str) -> Result<(), AppCommandError> {
    let payload = serde_json::json!({
        "type": "auth",
        "token": token,
        "audio": {
            "format": "pcm_s16le",
            "sampleRate": 16_000,
            "bitsPerSample": 16,
            "channels": 1,
        },
        "language": "zh-CN",
        "options": {
            "punctuation": true,
            "interimResults": true,
            "wordTimestamps": false,
        },
    });
    socket
        .send(Message::Text(payload.to_string().into()))
        .await
        .map_err(|error| {
            AppCommandError::network("Realtime voice authentication failed")
                .with_detail(error.to_string())
        })
}

async fn wait_for_ready(socket: &mut VoiceSocket) -> Result<(), AppCommandError> {
    tokio::time::timeout(READY_TIMEOUT, wait_for_ready_event(socket))
        .await
        .map_err(|_| AppCommandError::network("Realtime voice authentication timed out"))?
}

async fn wait_for_ready_event(socket: &mut VoiceSocket) -> Result<(), AppCommandError> {
    while let Some(message) = socket.next().await {
        let message = message.map_err(|error| {
            AppCommandError::network("Realtime voice authentication failed")
                .with_detail(error.to_string())
        })?;
        match parse_event(message).map_err(|detail| {
            AppCommandError::configuration_invalid("Invalid realtime voice response")
                .with_detail(detail)
        })? {
            Some(GatewayEvent::Ready { session_id }) if !session_id.trim().is_empty() => {
                tracing::info!("[RealtimeVoice] Fusion session ready");
                return Ok(());
            }
            Some(GatewayEvent::Error { code, message }) => {
                let detail = format!("{code}: {message}");
                return if code == "VOICE_AUTH_FAILED" {
                    Err(AppCommandError::authentication_failed(
                        "Realtime voice authentication was rejected",
                    )
                    .with_detail(detail))
                } else {
                    Err(
                        AppCommandError::network("Realtime voice service is unavailable")
                            .with_detail(detail),
                    )
                };
            }
            Some(_) | None => continue,
        }
    }
    Err(AppCommandError::network(
        "Realtime voice connection closed before authentication",
    ))
}
