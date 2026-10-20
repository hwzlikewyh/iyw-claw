use std::time::Duration;

use futures_util::{stream, StreamExt};

use super::{
    payload::{self, Envelope, Payload},
    Runtime,
};
use crate::chat_channel::{
    error::ChatChannelError, manager::ChatChannelManager, types::SentMessageId,
};
use crate::db::{
    entities::chat_channel_outbox::Model, service::chat_channel_outbox_service as storage,
};

const SEND_TIMEOUT: Duration = Duration::from_secs(120);
const CHANNEL_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_BASE_SECONDS: i64 = 10;
const RETRY_MAX_EXPONENT: i32 = 5;

pub(super) async fn sweep(manager: &ChatChannelManager, runtime: &Runtime) {
    if let Err(error) = storage::expire_sending(&runtime.db).await {
        tracing::warn!(error = %error, "[channel-delivery] interrupted send recovery failed");
    }
    let items = match storage::list_deliveries(&runtime.db).await {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(error = %error, "[channel-delivery] queue read failed");
            return;
        }
    };
    stream::iter(items)
        .for_each_concurrent(Some(4), |item| retry(manager, runtime, item))
        .await;
}

async fn retry(manager: &ChatChannelManager, runtime: &Runtime, item: Model) {
    let delay = RETRY_BASE_SECONDS
        .saturating_mul(2_i64.pow(item.attempts.clamp(0, RETRY_MAX_EXPONENT) as u32));
    if (chrono::Utc::now() - item.updated_at).num_seconds() < delay {
        return;
    }
    let Ok(envelope) = serde_json::from_str::<Envelope>(&item.content) else {
        return;
    };
    if envelope
        .progress
        .as_deref()
        .is_some_and(|id| crate::chat_channel::task_context::tracked(id).is_none())
    {
        let _ = storage::remove_pending(&runtime.db, item.id).await;
        return;
    }
    if item.state == "delivery_context" {
        let current = payload::context(runtime, &envelope.target)
            .await
            .map(|value| payload::hash(value.as_bytes()));
        if current.is_none() || current == envelope.context {
            return;
        }
    }
    let _ = deliver(manager, runtime, (item, envelope)).await;
}

pub(super) async fn deliver(
    manager: &ChatChannelManager,
    runtime: &Runtime,
    delivery: (Model, Envelope),
) -> Result<SentMessageId, ChatChannelError> {
    let (item, mut envelope) = delivery;
    let _permit = runtime
        .workers
        .acquire()
        .await
        .map_err(|_| payload::failure("DELIVERY_UNAVAILABLE"))?;
    if !matches!(
        storage::has_earlier_delivery(&runtime.db, &item).await,
        Ok(false)
    ) {
        return Err(ChatChannelError::DeliveryDeferred(format!(
            "CHANNEL_DELIVERY_QUEUED:{}",
            item.id
        )));
    }
    if !matches!(
        storage::claim_delivery(&runtime.db, item.id).await,
        Ok(true)
    ) {
        return Err(ChatChannelError::DeliveryDeferred(format!(
            "CHANNEL_DELIVERY_QUEUED:{}",
            item.id
        )));
    }
    let result = tokio::time::timeout(SEND_TIMEOUT, attempt(manager, runtime, &mut envelope))
        .await
        .unwrap_or_else(|_| {
            Err(ChatChannelError::SendFailed(
                "DELIVERY_UNKNOWN:TIMEOUT".into(),
            ))
        });
    finish(runtime, (item, envelope), result).await
}

async fn finish(
    runtime: &Runtime,
    delivery: (Model, Envelope),
    result: Result<SentMessageId, ChatChannelError>,
) -> Result<SentMessageId, ChatChannelError> {
    let (item, envelope) = delivery;
    let state = state(&result);
    let encoded =
        serde_json::to_string(&envelope).map_err(|_| payload::failure("INVALID_DELIVERY"))?;
    let error = result.as_ref().err().map(error_context);
    storage::finish_delivery(
        &runtime.db,
        item.clone(),
        (state, &encoded, error.as_deref()),
    )
    .await
    .map_err(|_| payload::failure("DELIVERY_RECEIPT_SAVE_FAILED"))?;
    record(runtime, (&item, &envelope), state).await;
    if state == "delivery_sent" {
        payload::cleanup(runtime, &envelope).await;
        if let Err(error) = storage::remove(&runtime.db, item.id).await {
            tracing::warn!(outbox_id = item.id, error = %error, "[channel-delivery] receipt cleanup failed");
        }
    }
    match state {
        "delivery_pending" | "delivery_context" => Err(ChatChannelError::DeliveryDeferred(
            format!("CHANNEL_DELIVERY_QUEUED:{}", item.id),
        )),
        _ => result,
    }
}

fn error_context(error: &ChatChannelError) -> String {
    let message = error.to_string();
    if let Some((_, suffix)) = message.split_once("code=") {
        let code: String = suffix
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
            .collect();
        if !code.is_empty() {
            return format!("{}:provider_code={code}", error.category());
        }
    }
    let code = message.split_whitespace().find(|word| {
        word.len() > 3
            && word.chars().all(|ch| {
                ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | ':' | '-')
            })
    });
    code.map(|code| {
        format!(
            "{}:{}",
            error.category(),
            code.chars().take(128).collect::<String>()
        )
    })
    .unwrap_or_else(|| error.category().into())
}

async fn attempt(
    manager: &ChatChannelManager,
    runtime: &Runtime,
    envelope: &mut Envelope,
) -> Result<SentMessageId, ChatChannelError> {
    // 与渠道重连/凭据更新共用门闩，校验后直至本次发送结束都不切换账号。
    let _channel = tokio::time::timeout(
        CHANNEL_LOCK_TIMEOUT,
        crate::chat_channel::operation_lock::lock_channel(envelope.target.channel_id),
    )
    .await
    .map_err(|_| ChatChannelError::DeliveryDeferred("CHANNEL_RECONFIGURING".into()))?;
    let target = payload::target(runtime, envelope).await?;
    if envelope
        .progress
        .as_deref()
        .is_some_and(|id| crate::chat_channel::task_context::tracked(id).is_none())
    {
        return Err(ChatChannelError::ConfigurationInvalid(
            "DELIVERY_PROGRESS_RETIRED".into(),
        ));
    }
    envelope.context = target
        .provider_payload
        .as_ref()
        .and_then(|value| value["context_token"].as_str())
        .map(|token| payload::hash(token.as_bytes()));
    let backend = manager.backend_for(target.channel_id).await?;
    // 飞书/钉钉等 HTTP 出站不依赖入站 WS 健康；由各 backend 判断实际发送能力。
    match &envelope.payload {
        Payload::Text { message } => backend.send_rich_message_to(&message.rich(), &target).await,
        file @ Payload::File { .. } => {
            backend
                .send_attachment_to(&payload::file(runtime, file).await?, &target)
                .await
        }
    }
}

fn state(result: &Result<SentMessageId, ChatChannelError>) -> &'static str {
    match result {
        Ok(_) => "delivery_sent",
        Err(ChatChannelError::DeliveryDeferred(message))
            if message.contains("CONTEXT") || message.contains("WEIXIN") =>
        {
            "delivery_context"
        }
        Err(
            ChatChannelError::DeliveryDeferred(_)
            | ChatChannelError::NotConnected
            | ChatChannelError::NotFound(_)
            | ChatChannelError::AuthenticationFailed(_),
        ) => "delivery_pending",
        Err(ChatChannelError::ConfigurationInvalid(_) | ChatChannelError::Unsupported(_)) => {
            "delivery_blocked"
        }
        Err(ChatChannelError::SendFailed(message))
            if message.contains("PROVIDER_REJECTED")
                && (message.contains("rate limit") || message.contains("频率")) =>
        {
            "delivery_pending"
        }
        Err(ChatChannelError::SendFailed(message))
            if message.contains("PROVIDER_REJECTED") || message.contains("Provider rejected") =>
        {
            "delivery_blocked"
        }
        Err(_) => "delivery_unknown",
    }
}

async fn record(runtime: &Runtime, entry: (&Model, &Envelope), state: &str) {
    let (item, envelope) = entry;
    if item.last_error.is_none() || item.state != state {
        tracing::info!(
            channel_id = item.channel_id,
            outbox_id = item.id,
            state,
            "[channel-delivery] delivery state"
        );
    }
    let summary = match &envelope.payload {
        Payload::Text { message } => message.rich().to_plain_text(),
        Payload::File { name, .. } => name.clone(),
    };
    let status = match state {
        "delivery_sent" => "sent",
        "delivery_pending" | "delivery_context" => "queued",
        "delivery_unknown" => "unknown",
        _ => "failed",
    };
    let _ = crate::db::service::chat_channel_message_log_service::create_log_for_target(
        &runtime.db,
        item.channel_id,
        "outbound",
        "delivery",
        &summary,
        status,
        (status != "sent").then(|| state.into()),
        Some(format!("outbox:{}", item.id)),
        None,
        Some(envelope.target_id.clone()),
    )
    .await;
}
