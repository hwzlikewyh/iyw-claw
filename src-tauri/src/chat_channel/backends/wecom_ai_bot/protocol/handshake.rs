use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use super::{
    connection_error, message_json, provider_error, request_id, send_stream_json, target_header,
    WsStream,
};
use crate::chat_channel::error::ChatChannelError;

const AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(crate) async fn connect_and_subscribe(
    endpoint: &str,
    bot_id: &str,
    secret: &str,
) -> Result<WsStream, ChatChannelError> {
    let (mut stream, _) = tokio_tungstenite::connect_async(endpoint)
        .await
        .map_err(connection_error)?;
    let req_id = request_id("aibot_subscribe");
    send_stream_json(&mut stream, subscribe_frame(&req_id, bot_id, secret)).await?;
    tokio::time::timeout(AUTH_TIMEOUT, wait_for_ack(&mut stream, &req_id))
        .await
        .map_err(|_| {
            ChatChannelError::AuthenticationFailed("subscribe acknowledgement timed out".into())
        })??;
    Ok(stream)
}

pub(crate) async fn verify_connection(
    endpoint: &str,
    bot_id: &str,
    secret: &str,
) -> Result<(), ChatChannelError> {
    let mut stream = connect_and_subscribe(endpoint, bot_id, secret).await?;
    let _ = stream.close(None).await;
    Ok(())
}

async fn wait_for_ack(stream: &mut WsStream, req_id: &str) -> Result<(), ChatChannelError> {
    loop {
        let message = stream
            .next()
            .await
            .ok_or_else(|| {
                ChatChannelError::ConnectionFailed("connection closed during subscribe".into())
            })?
            .map_err(connection_error)?;
        match message {
            Message::Ping(payload) => {
                stream
                    .send(Message::Pong(payload))
                    .await
                    .map_err(connection_error)?;
            }
            Message::Pong(_) => {}
            other => {
                let response = message_json(other)?;
                if target_header(&response, "req_id") != Some(req_id) {
                    continue;
                }
                if ack_code(&response) != Some(0) {
                    return Err(ChatChannelError::AuthenticationFailed(provider_error(
                        &response,
                    )));
                }
                return Ok(());
            }
        }
    }
}

fn subscribe_frame(req_id: &str, bot_id: &str, secret: &str) -> Value {
    json!({
        "cmd": "aibot_subscribe",
        "headers": { "req_id": req_id },
        "body": {
            "bot_id": bot_id,
            "secret": secret,
        },
    })
}
