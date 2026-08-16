use futures_util::SinkExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::{ChannelMessageTarget, IncomingCommand};

pub(super) async fn handle_frame<S>(
    channel_id: i32,
    raw: &str,
    write: &mut S,
    command_tx: &mpsc::Sender<IncomingCommand>,
    client: &reqwest::Client,
) -> Result<(), ChatChannelError>
where
    S: futures_util::Sink<tungstenite::Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let frame = parse_frame(raw, channel_id)?;
    if frame_type(&frame) == "SYSTEM" {
        return handle_system_frame(&frame, write).await;
    }
    if frame_type(&frame) != "CALLBACK" {
        return Ok(());
    }
    let Some(command) = parse_incoming(channel_id, &frame) else {
        acknowledge(&frame, write).await?;
        return Ok(());
    };
    match command_tx.try_send(command) {
        Ok(()) => acknowledge(&frame, write).await?,
        Err(mpsc::error::TrySendError::Full(command)) => {
            tracing::warn!(channel_id, "[DingTalk] dispatcher queue full");
            send_busy(client, &command.target).await.map_err(|error| {
                tracing::warn!(channel_id, error = %error, "[DingTalk] failed to report dispatcher congestion");
                error
            })?;
            acknowledge(&frame, write).await?;
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return Err(ChatChannelError::Other(
                "DingTalk command channel closed".into(),
            ));
        }
    }
    Ok(())
}

pub(super) async fn handle_registration_frame<S>(
    raw: &str,
    write: &mut S,
    channel_id: i32,
) -> Result<bool, ChatChannelError>
where
    S: futures_util::Sink<tungstenite::Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let frame = parse_frame(raw, channel_id)?;
    if frame_type(&frame) != "SYSTEM" {
        return Ok(false);
    }
    match topic(&frame) {
        "REGISTERED" => Ok(true),
        "ping" => {
            acknowledge_system(&frame, write).await?;
            Ok(false)
        }
        "disconnect" => {
            acknowledge_system(&frame, write).await?;
            Err(ChatChannelError::ConnectionFailed(
                "DingTalk registration was rejected".into(),
            ))
        }
        _ => Ok(false),
    }
}

async fn handle_system_frame<S>(
    frame: &serde_json::Value,
    write: &mut S,
) -> Result<(), ChatChannelError>
where
    S: futures_util::Sink<tungstenite::Message> + Unpin,
    S::Error: std::fmt::Display,
{
    match topic(frame) {
        "ping" => acknowledge_system(frame, write).await,
        "disconnect" => {
            acknowledge_system(frame, write).await?;
            Err(ChatChannelError::ConnectionFailed(
                "DingTalk requested stream disconnect".into(),
            ))
        }
        _ => Ok(()),
    }
}

async fn send_busy(
    client: &reqwest::Client,
    target: &ChannelMessageTarget,
) -> Result<(), ChatChannelError> {
    let payload = target.provider_payload.as_ref().ok_or_else(|| {
        ChatChannelError::ConfigurationInvalid("DingTalk reply context is missing".into())
    })?;
    if webhook_expired(payload) {
        return Err(ChatChannelError::SendFailed(
            "DingTalk session webhook expired".into(),
        ));
    }
    let webhook = string_field(payload, "session_webhook")?;
    let response = client
        .post(webhook)
        .json(&serde_json::json!({
            "msgtype": "markdown",
            "markdown": {
                "title": "iyw-claw",
                "text": crate::chat_channel::backends::DISPATCHER_BUSY_TEXT,
            },
        }))
        .send()
        .await
        .map_err(|error| ChatChannelError::SendFailed(super::redact_transport_error(&error)))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(ChatChannelError::SendFailed(format!(
            "DingTalk busy reply failed (HTTP {})",
            response.status()
        )))
    }
}

fn parse_incoming(channel_id: i32, frame: &serde_json::Value) -> Option<IncomingCommand> {
    let data = parse_data(frame.get("data")?)?;
    let text = data.pointer("/text/content")?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    let sender_id = field(&data, "senderStaffId").or_else(|| field(&data, "senderId"))?;
    let chat_id = if field(&data, "conversationType").as_deref() == Some("2") {
        field(&data, "conversationId").unwrap_or_else(|| sender_id.clone())
    } else {
        sender_id.clone()
    };
    let message_id = field(&data, "msgId")
        .or_else(|| field(&data, "messageId"))
        .or_else(|| {
            frame
                .pointer("/headers/messageId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    let webhook = field(&data, "sessionWebhook")?;
    let expires = data
        .get("sessionWebhookExpiredTime")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Some(IncomingCommand {
        channel_id,
        sender_id,
        sender_name: field(&data, "senderNick"),
        command_text: text.to_string(),
        callback_data: None,
        target: ChannelMessageTarget {
            channel_id,
            chat_id: Some(chat_id),
            thread_key: None,
            thread_kind: Some("dingtalk_chat".into()),
            provider_payload: Some(serde_json::json!({
                "session_webhook": webhook,
                "session_webhook_expired_time": expires,
            })),
        },
        metadata: serde_json::json!({}),
        message_trace_id: super::super::super::dedupe::new_message_trace_id(channel_id),
        provider_message_id: message_id,
        received_at: chrono::Utc::now(),
    })
}

async fn acknowledge<S>(frame: &serde_json::Value, write: &mut S) -> Result<(), ChatChannelError>
where
    S: futures_util::Sink<tungstenite::Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let message_id = frame
        .pointer("/headers/messageId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let ack = serde_json::json!({
        "code": 200,
        "headers": {
            "contentType": "application/json",
            "messageId": message_id,
        },
        "message": "OK",
        "data": "",
    });
    write_json(write, ack).await
}

async fn acknowledge_system<S>(
    frame: &serde_json::Value,
    write: &mut S,
) -> Result<(), ChatChannelError>
where
    S: futures_util::Sink<tungstenite::Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let ack = serde_json::json!({
        "code": 200,
        "headers": frame.get("headers").cloned().unwrap_or_else(|| serde_json::json!({})),
        "message": "OK",
        "data": frame.get("data").cloned().unwrap_or_else(|| serde_json::json!("")),
    });
    write_json(write, ack).await
}

async fn write_json<S>(write: &mut S, value: serde_json::Value) -> Result<(), ChatChannelError>
where
    S: futures_util::Sink<tungstenite::Message> + Unpin,
    S::Error: std::fmt::Display,
{
    write
        .send(tungstenite::Message::Text(value.to_string().into()))
        .await
        .map_err(|error| ChatChannelError::ConnectionFailed(super::redact_transport_error(&error)))
}

fn parse_frame(raw: &str, channel_id: i32) -> Result<serde_json::Value, ChatChannelError> {
    serde_json::from_str(raw).map_err(|error| {
        tracing::warn!(channel_id, %error, "[DingTalk] invalid frame");
        ChatChannelError::ConnectionFailed("DingTalk returned invalid JSON".into())
    })
}

fn frame_type(frame: &serde_json::Value) -> &str {
    frame
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn topic(frame: &serde_json::Value) -> &str {
    frame
        .pointer("/headers/topic")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn parse_data(value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::String(raw) => serde_json::from_str(raw).ok(),
        serde_json::Value::Object(_) => Some(value.clone()),
        _ => None,
    }
}

fn field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn webhook_expired(payload: &serde_json::Value) -> bool {
    let Some(expires) = payload.get("session_webhook_expired_time") else {
        return true;
    };
    let expires = expires.as_u64().or_else(|| expires.as_str()?.parse().ok());
    match expires {
        Some(value) => value <= chrono::Utc::now().timestamp_millis() as u64,
        None => true,
    }
}

pub(super) fn string_field<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> Result<&'a str, ChatChannelError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid(format!(
                "DingTalk target field `{key}` is missing"
            ))
        })
}
