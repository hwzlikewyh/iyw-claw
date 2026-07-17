use std::sync::Arc;

use tokio::sync::{mpsc, watch, Mutex};

use crate::chat_channel::types::{ChannelConnectionStatus, IncomingCommand};

use super::api::redact_token;
use super::format::{
    json_scalar_to_string, message_chat_matches, message_target, should_process_text,
    strip_bot_mention,
};

pub(super) struct PollContext {
    pub client: reqwest::Client,
    pub bot_token: String,
    pub configured_chat_id: String,
    pub bot_username: String,
    pub channel_id: i32,
    pub topic_mode: bool,
    pub status: Arc<Mutex<ChannelConnectionStatus>>,
}

pub(super) async fn run(
    context: PollContext,
    command_tx: mpsc::Sender<IncomingCommand>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut offset = 0_i64;
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        match poll_once(&context, offset, &mut shutdown_rx).await {
            PollResult::Updates(updates) => {
                mark_connected(&context.status).await;
                for update in updates {
                    offset = update
                        .get("update_id")
                        .and_then(serde_json::Value::as_i64)
                        .map_or(offset, |id| id + 1);
                    dispatch_update(&context, &command_tx, update).await;
                }
            }
            PollResult::Error(error) => {
                tracing::error!("[Telegram] polling error: {error}");
                *context.status.lock().await = ChannelConnectionStatus::Error;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            PollResult::Shutdown => break,
        }
    }
    *context.status.lock().await = ChannelConnectionStatus::Disconnected;
}

enum PollResult {
    Updates(Vec<serde_json::Value>),
    Error(String),
    Shutdown,
}

async fn poll_once(
    context: &PollContext,
    offset: i64,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> PollResult {
    let url = format!(
        "https://api.telegram.org/bot{}/getUpdates",
        context.bot_token
    );
    let body = serde_json::json!({
        "timeout": 30,
        "offset": offset,
        "allowed_updates": ["message", "callback_query"],
    });
    let response = tokio::select! {
        result = context.client.post(url).json(&body).send() => result,
        _ = shutdown_rx.changed() => return PollResult::Shutdown,
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return PollResult::Error(redact_token(error.to_string(), &context.bot_token))
        }
    };
    match response.json::<serde_json::Value>().await {
        Ok(body) => PollResult::Updates(
            body.get("result")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        ),
        Err(error) => PollResult::Error(redact_token(error.to_string(), &context.bot_token)),
    }
}

async fn dispatch_update(
    context: &PollContext,
    command_tx: &mpsc::Sender<IncomingCommand>,
    update: serde_json::Value,
) {
    if let Some(message) = update.get("message") {
        dispatch_message(context, command_tx, &update, message).await;
    } else if let Some(callback) = update.get("callback_query") {
        dispatch_callback(context, command_tx, &update, callback).await;
    }
}

async fn dispatch_message(
    context: &PollContext,
    command_tx: &mpsc::Sender<IncomingCommand>,
    update: &serde_json::Value,
    message: &serde_json::Value,
) {
    if !message_chat_matches(message, &context.configured_chat_id) {
        return;
    }
    let Some(text) = message.get("text").and_then(serde_json::Value::as_str) else {
        return;
    };
    let chat_type = message
        .pointer("/chat/type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("private");
    if !should_process_text(chat_type, text, &context.bot_username, context.topic_mode) {
        return;
    }
    let command = IncomingCommand {
        channel_id: context.channel_id,
        sender_id: sender_id(message),
        command_text: strip_bot_mention(text, &context.bot_username),
        callback_data: None,
        target: message_target(
            context.channel_id,
            &context.configured_chat_id,
            context.topic_mode,
            message,
        ),
        metadata: update.clone(),
    };
    send_command(command_tx, command).await;
}

async fn dispatch_callback(
    context: &PollContext,
    command_tx: &mpsc::Sender<IncomingCommand>,
    update: &serde_json::Value,
    callback: &serde_json::Value,
) {
    let Some(message) = callback.get("message") else {
        return;
    };
    if !message_chat_matches(message, &context.configured_chat_id) {
        return;
    }
    if let Some(id) = callback.get("id").and_then(serde_json::Value::as_str) {
        answer_callback_query(context, id).await;
    }
    let Some(data) = callback.get("data").and_then(serde_json::Value::as_str) else {
        return;
    };
    let command = IncomingCommand {
        channel_id: context.channel_id,
        sender_id: callback
            .pointer("/from/id")
            .and_then(json_scalar_to_string)
            .unwrap_or_default(),
        command_text: data.to_string(),
        callback_data: Some(data.to_string()),
        target: message_target(
            context.channel_id,
            &context.configured_chat_id,
            context.topic_mode,
            message,
        ),
        metadata: update.clone(),
    };
    send_command(command_tx, command).await;
}

async fn answer_callback_query(context: &PollContext, callback_id: &str) {
    let url = format!(
        "https://api.telegram.org/bot{}/answerCallbackQuery",
        context.bot_token
    );
    let result = context
        .client
        .post(url)
        .json(&serde_json::json!({ "callback_query_id": callback_id }))
        .send()
        .await;
    if let Err(error) = result {
        let message = redact_token(error.to_string(), &context.bot_token);
        tracing::warn!("[Telegram] answerCallbackQuery failed: {message}");
    }
}

fn sender_id(message: &serde_json::Value) -> String {
    message
        .pointer("/from/id")
        .and_then(json_scalar_to_string)
        .unwrap_or_default()
}

async fn send_command(command_tx: &mpsc::Sender<IncomingCommand>, command: IncomingCommand) {
    if let Err(error) = command_tx.send(command).await {
        tracing::error!("[Telegram] command dispatch failed: {error}");
    }
}

async fn mark_connected(status: &Mutex<ChannelConnectionStatus>) {
    let mut current = status.lock().await;
    if *current == ChannelConnectionStatus::Error {
        *current = ChannelConnectionStatus::Connected;
    }
}
