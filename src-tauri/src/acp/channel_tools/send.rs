use std::path::Path;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use serde_json::{json, Value};

use super::send_files::safe_send_digest;
use super::send_item::SentContent;
use super::send_result::{add_field, batch_status, map_send_error, result_index, with_index};
use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, SendItemInput, SendMessagesInput};
use crate::chat_channel::reply_routes::ReplySource;
use crate::chat_channel::types::RichMessage;
use crate::db::service::{chat_channel_message_log_service, chat_channel_service};

const SEND_WORKERS: usize = 8;
const SEND_ITEM_TIMEOUT: Duration = Duration::from_secs(60);
const MEDIA_FILE_TIMEOUT_SECS: u64 = 120;

impl ChannelToolService {
    pub(super) async fn send_messages(
        &self,
        caller: ChannelCaller,
        input: Result<SendMessagesInput, String>,
    ) -> Result<Value, String> {
        self.send_messages_with_source(caller, input, None).await
    }

    pub(super) async fn send_messages_with_source(
        &self,
        caller: ChannelCaller,
        input: Result<SendMessagesInput, String>,
        source: Option<ReplySource>,
    ) -> Result<Value, String> {
        let input = input?;
        if input.items.is_empty() {
            return Err("SEND_ITEMS_REQUIRED".to_string());
        }
        let digest = safe_send_digest(&input);
        let start = self
            .begin_mutation(&caller, "send_channel_messages", &input.request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let request_id = input.request_id;
        let mut results = self
            .dispatch_send_items(&caller, &request_id, (input.items, source))
            .await;
        results.sort_by_key(result_index);
        let result = json!({ "status": batch_status(&results), "items": results });
        self.finish_mutation(
            &caller,
            "send_channel_messages",
            &request_id,
            model,
            result,
            None,
        )
        .await
    }

    async fn dispatch_send_items(
        &self,
        caller: &ChannelCaller,
        request_id: &str,
        batch: (Vec<SendItemInput>, Option<ReplySource>),
    ) -> Vec<Value> {
        let (items, source) = batch;
        let working_dir = caller.working_dir.clone();
        stream::iter(items.into_iter().enumerate())
            .map(|(index, item)| {
                let caller = caller.clone();
                let request_id = request_id.to_string();
                let working_dir = working_dir.clone();
                let source = source.clone();
                async move {
                    let mut result = match tokio::time::timeout(
                        SEND_ITEM_TIMEOUT
                            + Duration::from_secs(
                                MEDIA_FILE_TIMEOUT_SECS * item.files.len() as u64,
                            ),
                        self.send_one(index, &item, (&working_dir, source.as_ref())),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => json!({
                            "index": index,
                            "status": "unknown",
                            "error": "CHANNEL_DELIVERY_UNKNOWN",
                        }),
                    };
                    if self
                        .audit_send_item(&caller, &request_id, &item, &working_dir, &result)
                        .await
                        .is_err()
                    {
                        add_field(&mut result, "audit_error", json!("AUDIT_UNAVAILABLE"));
                    }
                    result
                }
            })
            .buffer_unordered(SEND_WORKERS)
            .collect::<Vec<_>>()
            .await
    }

    async fn send_one(
        &self,
        index: usize,
        item: &SendItemInput,
        context: (&Path, Option<&ReplySource>),
    ) -> Value {
        match self.send_item_inner(item, context).await {
            Ok(value) => with_index(value, index),
            Err(code) => json!({ "index": index, "status": "failed", "error": code }),
        }
    }

    pub(super) async fn ensure_sendable_channel(&self, channel_id: i32) -> Result<(), String> {
        let channel = chat_channel_service::get_by_id(&self.db.conn, channel_id)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
        if !channel.enabled {
            return Err("CHANNEL_DISABLED".to_string());
        }
        Ok(())
    }

    pub(super) async fn send_content(
        &self,
        item: &SendItemInput,
        target: &crate::chat_channel::types::ChannelMessageTarget,
        target_id: &str,
        message: Option<RichMessage>,
    ) -> super::send_item::SentContent {
        let Some(message) = message else {
            return super::send_item::SentContent {
                delivered: false,
                message_id: None,
                log_error: None,
                error: None,
            };
        };
        if let Err(error) = self.manager.send_to_target(target, &message).await {
            return super::send_item::SentContent {
                delivered: false,
                message_id: None,
                log_error: None,
                error: Some(map_send_error(error)),
            };
        }
        self.record_sent_content(item, target_id, &message).await
    }

    async fn record_sent_content(
        &self,
        item: &SendItemInput,
        target_id: &str,
        message: &RichMessage,
    ) -> SentContent {
        let log = chat_channel_message_log_service::create_log_for_target_returning(
            &self.db.conn,
            item.channel_id,
            "outbound",
            if item.rich.is_some() { "rich" } else { "text" },
            &message.to_plain_text(),
            "sent",
            None,
            None,
            None,
            Some(target_id.to_string()),
        )
        .await;
        match log {
            Ok(log) => super::send_item::SentContent {
                delivered: true,
                message_id: Some(log.id),
                log_error: None,
                error: None,
            },
            Err(_) => super::send_item::SentContent {
                delivered: true,
                message_id: None,
                log_error: Some("MESSAGE_LOG_FAILED"),
                error: None,
            },
        }
    }
}
