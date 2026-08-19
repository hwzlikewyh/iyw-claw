use std::sync::Arc;

mod response;

use axum::body::{Body, Bytes};
use axum::extract::{Extension, Path, Query};
use axum::http::{Response, StatusCode};
use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;

use crate::app_state::AppState;
use crate::chat_channel::backends::{self, wecom_agent::crypto};
use crate::chat_channel::config_patch::mark_callback_verified as mark_callback_verified_config;
use crate::chat_channel::types::{
    ChannelMessageTarget, IncomingCommand, WecomAgentConfig, WecomAgentSecrets,
};
use crate::db::entities::chat_channel;
use crate::db::service::chat_channel_service;

use self::response::{callback_result, empty_response, text_response, xml_response, CallbackError};

#[derive(Debug, Deserialize)]
pub struct CallbackPath {
    channel_id: i32,
    callback_path: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    msg_signature: String,
    timestamp: String,
    nonce: String,
    echostr: Option<String>,
}

struct CallbackContext {
    model: chat_channel::Model,
    config: WecomAgentConfig,
    secrets: WecomAgentSecrets,
}

pub async fn verify_callback(
    Path(path): Path<CallbackPath>,
    Query(query): Query<CallbackQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Response<Body> {
    let request_id = uuid::Uuid::new_v4();
    let result = async {
        validate_query(&query)?;
        let context = load_context(&state, &path).await?;
        let encrypted = query.echostr.as_deref().ok_or(CallbackError::BadRequest)?;
        crypto::verify_signature(
            &query.msg_signature,
            &context.secrets.callback_token,
            &query.timestamp,
            &query.nonce,
            encrypted,
        )
        .map_err(|_| CallbackError::Unauthorized)?;
        let echo = crypto::decrypt(
            &context.secrets.encoding_aes_key,
            encrypted,
            &context.config.corp_id,
        )
        .map_err(|_| CallbackError::BadRequest)?;
        mark_callback_verified(&state, &context.model).await?;
        tracing::info!(channel_id = path.channel_id, %request_id, stage = "callback_verification", "[WeCom Agent] callback verified");
        Ok(text_response(StatusCode::OK, echo))
    }
    .await;
    callback_result(result, path.channel_id, request_id, "callback_verification")
}

pub async fn receive_callback(
    Path(path): Path<CallbackPath>,
    Query(query): Query<CallbackQuery>,
    Extension(state): Extension<Arc<AppState>>,
    body: Bytes,
) -> Response<Body> {
    let request_id = uuid::Uuid::new_v4();
    let result = receive_inner(&state, &path, &query, &body, request_id).await;
    callback_result(result, path.channel_id, request_id, "callback_receive")
}

async fn receive_inner(
    state: &AppState,
    path: &CallbackPath,
    query: &CallbackQuery,
    body: &[u8],
    request_id: uuid::Uuid,
) -> Result<Response<Body>, CallbackError> {
    validate_query(query)?;
    let context = load_context(state, path).await?;
    let body = std::str::from_utf8(body).map_err(|_| CallbackError::BadRequest)?;
    let envelope = crypto::parse_envelope(body).map_err(|_| CallbackError::BadRequest)?;
    crypto::verify_signature(
        &query.msg_signature,
        &context.secrets.callback_token,
        &query.timestamp,
        &query.nonce,
        &envelope.encrypt,
    )
    .map_err(|_| CallbackError::Unauthorized)?;
    let plaintext = crypto::decrypt(
        &context.secrets.encoding_aes_key,
        &envelope.encrypt,
        &context.config.corp_id,
    )
    .map_err(|_| CallbackError::BadRequest)?;
    let message = crypto::parse_message(&plaintext).map_err(|_| CallbackError::BadRequest)?;
    validate_message(&context.config, &message)?;
    if !context.model.enabled || !callback_ready(&context.config) {
        tracing::info!(channel_id = path.channel_id, %request_id, stage = "inbound_disabled", "[WeCom Agent] callback acknowledged while disabled");
        return Ok(empty_response(StatusCode::OK));
    }
    if message.msg_type != "text" || message.content.trim().is_empty() {
        tracing::info!(channel_id = path.channel_id, %request_id, message_type = message.msg_type, stage = "inbound_unsupported", "[WeCom Agent] unsupported callback acknowledged");
        return Ok(empty_response(StatusCode::OK));
    }
    let Some(provider_message_id) = nonempty(message.msg_id.clone()) else {
        tracing::warn!(channel_id = path.channel_id, %request_id, stage = "inbound_invalid", "[WeCom Agent] text callback missing MsgId");
        return Err(CallbackError::BadRequest);
    };
    let command = incoming_command(path.channel_id, &message, provider_message_id);
    match state
        .chat_channel_manager
        .command_sender()
        .try_send(command)
    {
        Ok(()) => {
            tracing::info!(channel_id = path.channel_id, %request_id, stage = "inbound_enqueued", "[WeCom Agent] text callback enqueued");
            Ok(empty_response(StatusCode::OK))
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            tracing::warn!(channel_id = path.channel_id, %request_id, stage = "inbound_queue_full", "[WeCom Agent] dispatcher queue full");
            busy_response(&context, &message)
        }
        Err(mpsc::error::TrySendError::Closed(_)) => Err(CallbackError::Unavailable),
    }
}

fn incoming_command(
    channel_id: i32,
    message: &crypto::WecomInboundMessage,
    provider_message_id: String,
) -> IncomingCommand {
    IncomingCommand {
        channel_id,
        sender_id: message.from_user_name.clone(),
        sender_name: None,
        command_text: message.content.clone(),
        callback_data: None,
        target: ChannelMessageTarget {
            channel_id,
            chat_id: Some(message.from_user_name.clone()),
            thread_key: None,
            thread_kind: Some("wecom_agent_user".to_string()),
            provider_payload: None,
        },
        metadata: serde_json::json!({}),
        message_trace_id: crate::chat_channel::dedupe::new_message_trace_id(channel_id),
        provider_message_id: Some(provider_message_id),
        received_at: Utc::now(),
    }
}

async fn load_context(
    state: &AppState,
    path: &CallbackPath,
) -> Result<CallbackContext, CallbackError> {
    let model = chat_channel_service::get_by_id(&state.db.conn, path.channel_id)
        .await
        .map_err(|_| CallbackError::Internal)?
        .filter(|model| model.channel_type == "wecom_agent")
        .ok_or(CallbackError::NotFound)?;
    let config: WecomAgentConfig =
        serde_json::from_str(&model.config_json).map_err(|_| CallbackError::BadRequest)?;
    if !constant_time_eq(&config.callback_path, &path.callback_path) {
        return Err(CallbackError::NotFound);
    }
    let raw = crate::keyring_store::get_channel_token(model.id).ok_or(CallbackError::BadRequest)?;
    let secrets = WecomAgentSecrets::parse(&raw).map_err(|_| CallbackError::BadRequest)?;
    Ok(CallbackContext {
        model,
        config,
        secrets,
    })
}

async fn mark_callback_verified(
    state: &AppState,
    model: &chat_channel::Model,
) -> Result<(), CallbackError> {
    let now = Utc::now();
    let next_config = mark_callback_verified_config(&model.config_json, &now.to_rfc3339())
        .map_err(|_| CallbackError::Internal)?;
    let result = chat_channel::Entity::update_many()
        .col_expr(chat_channel::Column::ConfigJson, Expr::value(next_config))
        .col_expr(chat_channel::Column::UpdatedAt, Expr::value(now))
        .filter(chat_channel::Column::Id.eq(model.id))
        .filter(chat_channel::Column::ConfigJson.eq(model.config_json.clone()))
        .exec(&state.db.conn)
        .await
        .map_err(|_| CallbackError::Internal)?;
    if result.rows_affected == 1 {
        Ok(())
    } else {
        Err(CallbackError::Unavailable)
    }
}

fn callback_ready(config: &WecomAgentConfig) -> bool {
    config.setup_state == "ready"
        && config
            .callback_verified_at
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
}

fn validate_query(query: &CallbackQuery) -> Result<(), CallbackError> {
    if query.msg_signature.len() != 40
        || query.timestamp.parse::<u64>().is_err()
        || query.nonce.is_empty()
        || query.nonce.len() > 128
    {
        return Err(CallbackError::BadRequest);
    }
    Ok(())
}

fn validate_message(
    config: &WecomAgentConfig,
    message: &crypto::WecomInboundMessage,
) -> Result<(), CallbackError> {
    if message.to_user_name != config.corp_id || message.agent_id != config.agent_id {
        return Err(CallbackError::Unauthorized);
    }
    Ok(())
}

fn busy_response(
    context: &CallbackContext,
    message: &crypto::WecomInboundMessage,
) -> Result<Response<Body>, CallbackError> {
    let timestamp = Utc::now().timestamp().to_string();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let plaintext = crypto::passive_text_xml(
        &message.from_user_name,
        &message.to_user_name,
        backends::DISPATCHER_BUSY_TEXT,
        Utc::now().timestamp(),
    )
    .map_err(|_| CallbackError::Internal)?;
    let encrypted = crypto::encrypt(
        &context.secrets.encoding_aes_key,
        &plaintext,
        &context.config.corp_id,
    )
    .map_err(|_| CallbackError::Internal)?;
    let signature = crypto::signature(
        &context.secrets.callback_token,
        &timestamp,
        &nonce,
        &encrypted,
    );
    let xml = crypto::encrypted_response_xml(&encrypted, &signature, &timestamp, &nonce)
        .map_err(|_| CallbackError::Internal)?;
    Ok(xml_response(StatusCode::OK, xml))
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    left.len() == right.len() && left.as_bytes().ct_eq(right.as_bytes()).unwrap_u8() == 1
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
