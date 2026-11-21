use tokio::sync::Mutex;

use super::{ChatChannelError, DatabaseConnection, SendRequest, WeixinBackend, WeixinReplyContext};
use crate::db::service::chat_channel_outbox_service;

pub(super) struct DrainRequest<'a> {
    pub client: &'a reqwest::Client,
    pub base_url: &'a str,
    pub bot_token: &'a str,
    pub database: &'a DatabaseConnection,
    pub channel_id: i32,
    pub to_user_id: &'a str,
    pub context_token: &'a str,
    pub reply_context: &'a Mutex<Option<WeixinReplyContext>>,
}

pub(super) async fn enqueue(
    database: &DatabaseConnection,
    channel_id: i32,
    recipient_id: &str,
    text: &str,
) -> Result<i32, ChatChannelError> {
    chat_channel_outbox_service::enqueue(database, channel_id, recipient_id, text)
        .await
        .map(|item| item.id)
        .map_err(|error| ChatChannelError::SendFailed(error.to_string()))
}

pub(super) async fn drain(request: DrainRequest<'_>) {
    let items = match chat_channel_outbox_service::list_waiting(
        request.database,
        request.channel_id,
        request.to_user_id,
    )
    .await
    {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(channel_id = request.channel_id, error = %error, "[Weixin] deferred queue read failed");
            return;
        }
    };
    for item in items {
        match chat_channel_outbox_service::claim_waiting_context(request.database, item.id).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                tracing::warn!(outbox_id = item.id, %error, "[Weixin] deferred message claim failed");
                break;
            }
        }
        // 旧版只保存 send 分类，无法区分拒绝和已经受理但丢失的回执。
        let result = if item.attempts > 0 && item.last_error.as_deref() == Some("send") {
            Err(ChatChannelError::SendFailed(
                "DELIVERY_UNKNOWN:LEGACY_WEIXIN_RESPONSE".into(),
            ))
        } else {
            send_item(&request, &item.content).await
        };
        match result {
            Ok(()) => {
                if let Err(error) =
                    chat_channel_outbox_service::remove(request.database, item.id).await
                {
                    tracing::warn!(channel_id = request.channel_id, outbox_id = item.id, error = %error, "[Weixin] deferred message cleanup failed");
                    break;
                }
            }
            Err(error) => {
                record_unsent(request.database, item, &error).await;
                break;
            }
        }
    }
}

async fn record_unsent(
    database: &DatabaseConnection,
    item: crate::db::entities::chat_channel_outbox::Model,
    error: &ChatChannelError,
) {
    let channel_id = item.channel_id;
    let state = match error {
        ChatChannelError::DeliveryDeferred(_)
        | ChatChannelError::AuthenticationFailed(_)
        | ChatChannelError::NotConnected => "waiting_context",
        ChatChannelError::SendFailed(reason) if reason.starts_with("PROVIDER_REJECTED:") => {
            "delivery_blocked"
        }
        _ => "delivery_unknown",
    };
    if let Err(store_error) =
        chat_channel_outbox_service::record_failure(database, item, (state, error.category())).await
    {
        tracing::warn!(error = %store_error, "[Weixin] deferred message state update failed");
    }
    tracing::warn!(
        channel_id,
        error_category = error.category(),
        state,
        "[Weixin] deferred delivery paused"
    );
}

async fn send_item(request: &DrainRequest<'_>, text: &str) -> Result<(), ChatChannelError> {
    WeixinBackend::do_send(SendRequest {
        client: request.client,
        base_url: request.base_url,
        bot_token: request.bot_token,
        to_user_id: request.to_user_id,
        context_token: request.context_token,
        item: super::protocol::text_item(text),
        database: request.database,
        channel_id: request.channel_id,
        reply_context: request.reply_context,
    })
    .await
    .map(|_| ())
}
