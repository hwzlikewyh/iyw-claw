mod handshake;

use futures_util::SinkExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::runtime::RunArgs;
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::*;

pub(crate) type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
pub(crate) type WsSink = futures_util::stream::SplitSink<WsStream, Message>;
pub(crate) use handshake::{connect_and_subscribe, verify_connection};

pub(crate) struct ProviderAck {
    pub(crate) req_id: String,
    pub(crate) error: Option<String>,
}

pub(crate) async fn handle_message(
    message: Message,
    write: &mut WsSink,
    args: &mut RunArgs,
) -> Result<Option<ProviderAck>, ChatChannelError> {
    if let Message::Ping(payload) = message {
        return write
            .send(Message::Pong(payload))
            .await
            .map(|_| None)
            .map_err(send_error);
    }
    let frame = message_json(message)?;
    if disconnect_event(&frame) {
        return Err(ChatChannelError::AlreadyConnected);
    }
    let cmd = frame["cmd"].as_str().unwrap_or_default();
    if cmd == "ping" {
        return write_json(write, pong_frame(target_header(&frame, "req_id")))
            .await
            .map(|_| None);
    }
    if cmd == "pong" {
        return Ok(None);
    }
    if !matches!(
        cmd,
        "aibot_msg_callback" | "aibot_callback" | "aibot_event_callback"
    ) {
        return Ok(provider_ack(&frame));
    }
    let Some(command) = parse_callback(&frame, args.channel_id) else {
        return Ok(None);
    };
    match args.command_tx.try_send(command) {
        Ok(()) => Ok(None),
        Err(mpsc::error::TrySendError::Full(command)) => {
            tracing::warn!(
                channel_id = args.channel_id,
                "[WeComAiBot] dispatcher queue full"
            );
            send_busy(write, &command.target).await.map(|_| None)
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            Err(ChatChannelError::Other("command channel closed".into()))
        }
    }
}

fn provider_ack(frame: &Value) -> Option<ProviderAck> {
    let req_id = target_header(frame, "req_id")?.to_string();
    let code = ack_code(frame)?;
    Some(ProviderAck {
        req_id,
        error: (code != 0).then(|| provider_error(frame)),
    })
}

fn parse_callback(frame: &Value, channel_id: i32) -> Option<IncomingCommand> {
    let body = frame.get("body")?;
    let text = callback_text(body)?;
    let (sender_id, chat_id) = callback_address(body)?;
    let req_id = target_header(frame, "req_id")
        .unwrap_or_default()
        .to_string();
    let mut provider_id = body
        .get("msgid")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or(&req_id)
        .to_string();
    if provider_id.is_empty() {
        provider_id = request_id("inbound");
    }
    Some(build_command(
        channel_id,
        sender_id,
        chat_id,
        text,
        req_id,
        provider_id,
        body["chattype"].clone(),
    ))
}

fn callback_text(body: &Value) -> Option<&str> {
    if body["msgtype"].as_str().is_some_and(|kind| kind != "text") {
        return None;
    }
    let text = body
        .pointer("/text/content")
        .or_else(|| body.get("content"))?
        .as_str()?
        .trim();
    (!text.is_empty()).then_some(text)
}

fn callback_address(body: &Value) -> Option<(&str, &str)> {
    let sender_id = body
        .pointer("/from/userid")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let chat_id = body
        .get("chatid")
        .and_then(Value::as_str)
        .unwrap_or(sender_id);
    (!chat_id.is_empty()).then_some((sender_id, chat_id))
}

fn build_command(
    channel_id: i32,
    sender_id: &str,
    chat_id: &str,
    text: &str,
    req_id: String,
    provider_id: String,
    chattype: Value,
) -> IncomingCommand {
    IncomingCommand {
        channel_id,
        sender_id: sender_id.to_string(),
        sender_name: None,
        command_text: text.to_string(),
        callback_data: None,
        target: ChannelMessageTarget {
            channel_id,
            chat_id: Some(chat_id.to_string()),
            thread_key: None,
            thread_kind: Some("wecom_ai_bot".into()),
            provider_payload: Some(json!({
                "req_id": req_id,
                "chatid": chat_id,
                "sender_id": sender_id,
                "chattype": chattype.clone(),
            })),
        },
        metadata: json!({ "chattype": chattype, "msgtype": "text" }),
        message_trace_id: crate::chat_channel::dedupe::new_message_trace_id(channel_id),
        provider_message_id: Some(provider_id),
        received_at: chrono::Utc::now(),
    }
}

async fn send_busy(
    write: &mut WsSink,
    target: &ChannelMessageTarget,
) -> Result<(), ChatChannelError> {
    let req_id = target_payload(target, "req_id");
    let chatid = target
        .chat_id
        .as_deref()
        .or_else(|| target_payload(target, "chatid"));
    let frame = match req_id.filter(|value| !value.is_empty()) {
        Some(req_id) => reply_frame(req_id, crate::chat_channel::backends::DISPATCHER_BUSY_TEXT),
        None => proactive_frame(
            chatid.unwrap_or_default(),
            crate::chat_channel::backends::DISPATCHER_BUSY_TEXT,
        ),
    };
    write_json(write, frame).await
}

pub(crate) fn reply_frame(req_id: &str, text: &str) -> Value {
    json!({ "cmd": "aibot_respond_msg", "headers": { "req_id": req_id }, "body": { "msgtype": "markdown", "markdown": { "content": text } } })
}

pub(crate) fn proactive_frame(chatid: &str, text: &str) -> Value {
    json!({ "cmd": "aibot_send_msg", "headers": { "req_id": request_id("aibot_send_msg") }, "body": { "chatid": chatid, "msgtype": "markdown", "markdown": { "content": text } } })
}

pub(crate) fn target_payload<'a>(target: &'a ChannelMessageTarget, key: &str) -> Option<&'a str> {
    target.provider_payload.as_ref()?.get(key)?.as_str()
}

pub(crate) fn ping_frame() -> Value {
    json!({ "cmd": "ping", "headers": { "req_id": request_id("ping") } })
}

fn pong_frame(req_id: Option<&str>) -> Value {
    let req_id = req_id
        .map(str::to_string)
        .unwrap_or_else(|| request_id("pong"));
    json!({ "cmd": "pong", "headers": { "req_id": req_id } })
}
fn request_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

fn target_header<'a>(frame: &'a Value, key: &str) -> Option<&'a str> {
    frame.get("headers")?.get(key)?.as_str()
}

pub(crate) fn frame_request_id(frame: &Value) -> Option<&str> {
    target_header(frame, "req_id").filter(|value| !value.is_empty())
}

fn ack_code(frame: &Value) -> Option<i64> {
    frame
        .get("errcode")
        .and_then(Value::as_i64)
        .or_else(|| frame.pointer("/headers/errcode").and_then(Value::as_i64))
        .or_else(|| frame.pointer("/body/errcode").and_then(Value::as_i64))
}

pub(super) fn provider_error(frame: &Value) -> String {
    let message = frame
        .get("errmsg")
        .and_then(Value::as_str)
        .or_else(|| frame.pointer("/headers/errmsg").and_then(Value::as_str))
        .or_else(|| frame.pointer("/body/errmsg").and_then(Value::as_str))
        .unwrap_or("request rejected");
    format!(
        "code={}, message={message}",
        ack_code(frame).unwrap_or_default()
    )
}

fn disconnect_event(frame: &Value) -> bool {
    frame.pointer("/body/eventtype").and_then(Value::as_str) == Some("disconnected_event")
        || frame
            .pointer("/body/event/eventtype")
            .and_then(Value::as_str)
            == Some("disconnected_event")
}

async fn send_stream_json(stream: &mut WsStream, frame: Value) -> Result<(), ChatChannelError> {
    stream
        .send(Message::Text(serialize(frame)?.into()))
        .await
        .map_err(connection_error)
}

pub(crate) async fn write_json(write: &mut WsSink, frame: Value) -> Result<(), ChatChannelError> {
    write
        .send(Message::Text(serialize(frame)?.into()))
        .await
        .map_err(send_error)
}

fn serialize(frame: Value) -> Result<String, ChatChannelError> {
    serde_json::to_string(&frame).map_err(|error| ChatChannelError::SendFailed(error.to_string()))
}

fn message_json(message: Message) -> Result<Value, ChatChannelError> {
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(data) => String::from_utf8(data.to_vec())
            .map_err(|_| ChatChannelError::Other("non-UTF8 WeCom frame".into()))?,
        Message::Close(_) => {
            return Err(ChatChannelError::ConnectionFailed(
                "WebSocket closed".into(),
            ))
        }
        Message::Pong(_) => return Ok(json!({})),
        Message::Ping(_) | Message::Frame(_) => {
            return Err(ChatChannelError::Other("unexpected WebSocket frame".into()))
        }
    };
    serde_json::from_str(&text)
        .map_err(|_| ChatChannelError::Other("invalid WeCom JSON frame".into()))
}

fn connection_error(error: tokio_tungstenite::tungstenite::Error) -> ChatChannelError {
    ChatChannelError::ConnectionFailed(redact_transport_error(&error))
}

fn send_error(error: tokio_tungstenite::tungstenite::Error) -> ChatChannelError {
    ChatChannelError::SendFailed(redact_transport_error(&error))
}

fn redact_transport_error(error: &impl std::fmt::Display) -> String {
    let message = error.to_string();
    match message.find('?') {
        Some(index) => format!("{}?[redacted]", &message[..index]),
        None => message,
    }
}
