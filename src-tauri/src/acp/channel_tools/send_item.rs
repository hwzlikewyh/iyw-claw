use std::path::Path;

use serde_json::{json, Value};

use super::send_files::{inspect_files, FileSendResult};
use super::send_result::{build_message, first_file_error, map_send_error, send_status};
use super::service::ChannelToolService;
use super::types::SendItemInput;
use crate::chat_channel::reply_routes::ReplySource;
use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::entities::chat_channel_target;
use crate::db::service::chat_channel_target_service;

pub(super) struct SentContent {
    pub(super) delivered: bool,
    pub(super) message_id: Option<i32>,
    pub(super) log_error: Option<&'static str>,
    pub(super) error: Option<String>,
}

impl ChannelToolService {
    pub(super) async fn send_item_inner(
        &self,
        item: &SendItemInput,
        context: (&Path, Option<&ReplySource>),
    ) -> Result<Value, String> {
        let (target_row, target, target_id) = self.resolve_send_target(item).await?;
        let capability = self
            .manager
            .attachment_capability(item.channel_id)
            .await
            .map_err(map_send_error)?;
        let files = inspect_files(&item.files, context.0, capability).await;
        let (message_requested, sent) = self
            .send_requested_content(item, (&target_id, &target), context.1)
            .await?;
        let file_results = self
            .send_files_to_target(item.channel_id, &target_id, &target, files)
            .await;
        let target_error = self
            .update_target_if_delivered(target_row, &sent, &file_results)
            .await;
        render_send_result(item, (message_requested, sent, file_results), target_error)
    }

    async fn resolve_send_target(
        &self,
        item: &SendItemInput,
    ) -> Result<(chat_channel_target::Model, ChannelMessageTarget, String), String> {
        validate_send_item(item)?;
        self.ensure_sendable_channel(item.channel_id).await?;
        let target_id = item.target_id.as_deref().ok_or("TARGET_NOT_FOUND")?;
        let (row, target) = lookup_target(&self.db.conn, item.channel_id, target_id).await?;
        Ok((row, target, target_id.to_string()))
    }

    async fn send_requested_content(
        &self,
        item: &SendItemInput,
        target: (&str, &ChannelMessageTarget),
        source: Option<&ReplySource>,
    ) -> Result<(bool, SentContent), String> {
        let message = build_message(item)?;
        let message_requested = message.is_some();
        let route = if message_requested {
            self.prepare_reply_route(source, target).await?
        } else {
            None
        };
        let sent = self.send_content(item, target.1, target.0, message).await;
        clear_failed_route(&self.manager, route, &sent);
        Ok((message_requested, sent))
    }

    async fn update_target_if_delivered(
        &self,
        target: chat_channel_target::Model,
        sent: &SentContent,
        files: &[FileSendResult],
    ) -> Option<&'static str> {
        let delivered = sent.delivered || files.iter().any(|file| file.status == "sent");
        if !delivered {
            return None;
        }
        chat_channel_target_service::touch(&self.db.conn, target)
            .await
            .err()
            .map(|_| "TARGET_UPDATE_FAILED")
    }
}

fn validate_send_item(item: &SendItemInput) -> Result<(), String> {
    if item.files.len() > crate::chat_channel::attachments::MAX_MESSAGE_ATTACHMENTS {
        return Err("TOO_MANY_ATTACHMENTS".into());
    }
    if item.files.is_empty() && item.text.is_none() && item.rich.is_none() {
        return Err("MESSAGE_CONTENT_REQUIRED".into());
    }
    Ok(())
}

async fn lookup_target(
    db: &sea_orm::DatabaseConnection,
    channel_id: i32,
    target_id: &str,
) -> Result<(chat_channel_target::Model, ChannelMessageTarget), String> {
    if chat_channel_target_service::find_by_public_target_id(db, target_id)
        .await
        .map_err(|_| "TARGET_QUERY_FAILED".to_string())?
        .is_some_and(|target| target.channel_id != channel_id)
    {
        return Err("TARGET_CHANNEL_MISMATCH".to_string());
    }
    chat_channel_target_service::resolve(db, channel_id, target_id)
        .await
        .map_err(|_| "TARGET_NOT_FOUND".to_string())
}

fn clear_failed_route(
    manager: &crate::chat_channel::manager::ChatChannelManager,
    route: Option<String>,
    sent: &SentContent,
) {
    let ambiguous = matches!(
        sent.error.as_deref(),
        Some("CHANNEL_DELIVERY_QUEUED" | "CHANNEL_DELIVERY_UNKNOWN")
    );
    if !sent.delivered && !ambiguous {
        if let Some(route) = route {
            manager.reply_routes().remove(&route);
        }
    }
}

fn render_send_result(
    item: &SendItemInput,
    result: (bool, SentContent, Vec<FileSendResult>),
    target_error: Option<&'static str>,
) -> Result<Value, String> {
    let (message_requested, sent, files) = result;
    let delivered = sent.delivered || files.iter().any(|file| file.status == "sent");
    let mut status = send_status(
        message_requested,
        sent.delivered,
        &files,
        item.files.is_empty(),
    )?;
    let message_queued = sent.error.as_deref() == Some("CHANNEL_DELIVERY_QUEUED");
    if message_queued || files.iter().any(|file| file.status == "queued") {
        status = if delivered {
            "partial_success"
        } else {
            "queued"
        };
    }
    let message_unknown = sent.error.as_deref() == Some("CHANNEL_DELIVERY_UNKNOWN");
    if message_unknown {
        status = if delivered {
            "partial_success"
        } else {
            "unknown"
        };
    }
    let file_error = first_file_error(&files);
    let log_error = sent
        .log_error
        .or_else(|| files.iter().find_map(|file| file.log_error));
    Ok(json!({
        "status": status,
        "channel_id": item.channel_id,
        "target_id": item.target_id,
        "message_id": sent.message_id.map(|id| format!("cm_{id}")),
        "message_error": sent.error,
        "message_status": if sent.delivered { "sent" } else if message_queued { "queued" } else if message_unknown { "unknown" } else if message_requested { "failed" } else { "skipped" },
        "log_error": log_error,
        "target_error": target_error,
        "files": files,
        "file_error": file_error,
    }))
}
