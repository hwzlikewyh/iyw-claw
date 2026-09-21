use tokio::sync::Mutex;

use super::{ChatChannelError, DatabaseConnection, SendRequest, WeixinBackend, WeixinReplyContext};
use crate::db::service::chat_channel_outbox_service;

pub(super) struct DrainRequest<'a> {
    pub client: &'a reqwest::Client,
    pub base_url: &'a str,
    pub bot_token: &'a str,
    pub wechat_uin: &'a str,
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
        let result = send_item(&request, &item.content).await;
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
                let _ = chat_channel_outbox_service::record_failure(
                    request.database,
                    item,
                    error.category(),
                )
                .await;
                tracing::warn!(
                    channel_id = request.channel_id,
                    error_category = error.category(),
                    "[Weixin] deferred delivery paused"
                );
                break;
            }
        }
    }
}

async fn send_item(request: &DrainRequest<'_>, text: &str) -> Result<(), ChatChannelError> {
    WeixinBackend::do_send(SendRequest {
        client: request.client,
        base_url: request.base_url,
        bot_token: request.bot_token,
        wechat_uin: request.wechat_uin,
        to_user_id: request.to_user_id,
        context_token: request.context_token,
        text,
        database: request.database,
        channel_id: request.channel_id,
        reply_context: request.reply_context,
    })
    .await
}
