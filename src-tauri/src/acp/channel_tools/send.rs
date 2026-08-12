use std::path::Path;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use serde_json::{json, Value};

use super::send_files::{inspect_files, safe_send_digest};
use super::send_result::{
    add_field, batch_status, build_message, first_file_error, map_send_error, result_index,
    send_status, with_index,
};
use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, SendItemInput, SendMessagesInput};
use crate::chat_channel::types::RichMessage;
use crate::db::service::{
    chat_channel_message_log_service, chat_channel_service, chat_channel_target_service,
};

const SEND_WORKERS: usize = 8;
const SEND_ITEM_TIMEOUT: Duration = Duration::from_secs(60);

struct SentContent {
    delivered: bool,
    message_id: Option<i32>,
    log_error: Option<&'static str>,
    error: Option<String>,
}

impl ChannelToolService {
    pub(super) async fn send_messages(
        &self,
        caller: ChannelCaller,
        input: Result<SendMessagesInput, String>,
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
            .dispatch_send_items(&caller, &request_id, input.items)
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
        items: Vec<SendItemInput>,
    ) -> Vec<Value> {
        let working_dir = caller.working_dir.clone();
        stream::iter(items.into_iter().enumerate())
            .map(|(index, item)| {
                let caller = caller.clone();
                let request_id = request_id.to_string();
                let working_dir = working_dir.clone();
                async move {
                    let mut result = match tokio::time::timeout(
                        SEND_ITEM_TIMEOUT,
                        self.send_one(index, &item, &working_dir),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => json!({
                            "index": index,
                            "status": "failed",
                            "error": "CHANNEL_SEND_TIMEOUT",
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

    async fn send_one(&self, index: usize, item: &SendItemInput, working_dir: &Path) -> Value {
        match self.send_one_inner(item, working_dir).await {
            Ok(value) => with_index(value, index),
            Err(code) => json!({ "index": index, "status": "failed", "error": code }),
        }
    }

    async fn send_one_inner(
        &self,
        item: &SendItemInput,
        working_dir: &Path,
    ) -> Result<Value, String> {
        self.ensure_sendable_channel(item.channel_id).await?;
        let target_id = item.target_id.as_deref().ok_or("TARGET_NOT_FOUND")?;
        if chat_channel_target_service::find_by_public_target_id(&self.db.conn, target_id)
            .await
            .map_err(|_| "TARGET_QUERY_FAILED".to_string())?
            .is_some_and(|target| target.channel_id != item.channel_id)
        {
            return Err("TARGET_CHANNEL_MISMATCH".to_string());
        }
        let (target_row, target) =
            chat_channel_target_service::resolve(&self.db.conn, item.channel_id, target_id)
                .await
                .map_err(|_| "TARGET_NOT_FOUND".to_string())?;
        let capability = self
            .manager
            .attachment_capability(item.channel_id)
            .await
            .map_err(map_send_error)?;
        let files = inspect_files(&item.files, working_dir, capability).await;
        let message = build_message(&item)?;
        let message_requested = message.is_some();
        let sent = self.send_content(item, &target, target_id, message).await;
        let file_results = self
            .send_files_to_target(item.channel_id, target_id, &target, files)
            .await;
        let delivered = sent.delivered || file_results.iter().any(|file| file.status == "sent");
        let target_error = if delivered {
            chat_channel_target_service::touch(&self.db.conn, target_row)
                .await
                .err()
                .map(|_| "TARGET_UPDATE_FAILED")
        } else {
            None
        };
        let status = send_status(
            message_requested,
            sent.delivered,
            &file_results,
            item.files.is_empty(),
        )?;
        let file_error = first_file_error(&file_results);
        let file_log_error = file_results.iter().find_map(|file| file.log_error);
        Ok(json!({
            "status": status,
            "channel_id": item.channel_id,
            "target_id": target_id,
            "message_id": sent.message_id.map(|id| format!("cm_{id}")),
            "message_error": sent.error,
            "log_error": sent.log_error.or(file_log_error),
            "target_error": target_error,
            "files": file_results,
            "file_error": file_error,
        }))
    }

    async fn ensure_sendable_channel(&self, channel_id: i32) -> Result<(), String> {
        let channel = chat_channel_service::get_by_id(&self.db.conn, channel_id)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
        if channel.runtime_status != "connected" || !self.manager.is_connected(channel_id).await {
            return Err("CHANNEL_NOT_CONNECTED".to_string());
        }
        Ok(())
    }

    async fn send_content(
        &self,
        item: &SendItemInput,
        target: &crate::chat_channel::types::ChannelMessageTarget,
        target_id: &str,
        message: Option<RichMessage>,
    ) -> SentContent {
        let Some(message) = message else {
            return SentContent {
                delivered: false,
                message_id: None,
                log_error: None,
                error: None,
            };
        };
        if let Err(error) = self.manager.send_to_target(target, &message).await {
            return SentContent {
                delivered: false,
                message_id: None,
                log_error: None,
                error: Some(map_send_error(error)),
            };
        }
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
            Ok(log) => SentContent {
                delivered: true,
                message_id: Some(log.id),
                log_error: None,
                error: None,
            },
            Err(_) => SentContent {
                delivered: true,
                message_id: None,
                log_error: Some("MESSAGE_LOG_FAILED"),
                error: None,
            },
        }
    }
}
