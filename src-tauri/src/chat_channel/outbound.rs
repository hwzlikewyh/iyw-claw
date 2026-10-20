//! 复用渠道 outbox 持久保存交付；连接、业务执行与投递状态分别管理。
#[path = "outbound/payload.rs"]
mod payload;
#[path = "outbound/spool.rs"]
mod spool;
#[path = "outbound/worker.rs"]
mod worker;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::sync::{Mutex, Semaphore};

use super::attachments::ChannelAttachment;
use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::types::{ChannelMessageTarget, RichMessage, SentMessageId};
use crate::db::service::chat_channel_outbox_service as storage;

pub(super) struct Runtime {
    db: DatabaseConnection,
    spool: PathBuf,
    enqueue: Mutex<()>,
    workers: Semaphore,
}

#[derive(Clone)]
pub(super) enum Content {
    Text(RichMessage),
    Progress(RichMessage, Option<String>),
    File(ChannelAttachment),
}

impl Runtime {
    pub fn new(db: DatabaseConnection, data_dir: PathBuf) -> Self {
        Self {
            db,
            spool: data_dir.join("channel-outbox"),
            enqueue: Mutex::new(()),
            workers: Semaphore::new(4),
        }
    }
}

pub(super) async fn attachment_capability(
    manager: &ChatChannelManager,
    channel_id: i32,
) -> Result<super::attachments::AttachmentCapability, ChatChannelError> {
    let runtime = manager
        .delivery_runtime()
        .ok_or(ChatChannelError::NotFound(channel_id))?;
    let channel = crate::db::service::chat_channel_service::get_by_id(&runtime.db, channel_id)
        .await
        .map_err(|_| payload::failure("CHANNEL_QUERY_FAILED"))?
        .ok_or(ChatChannelError::NotFound(channel_id))?;
    Ok(super::attachments::AttachmentCapability::for_channel(
        &channel.channel_type,
    ))
}

pub(super) async fn spawn(manager: ChatChannelManager, runtime: Arc<Runtime>) {
    if let Err(error) = storage::recover_deliveries(&runtime.db).await {
        tracing::error!(error = %error, "[channel-delivery] recovery failed");
        return;
    }
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(10));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            worker::sweep(&manager, &runtime).await;
        }
    });
}

pub(super) async fn send(
    manager: &ChatChannelManager,
    target: &ChannelMessageTarget,
    content: Content,
) -> Result<SentMessageId, ChatChannelError> {
    let Some(runtime) = manager.delivery_runtime() else {
        return direct(manager, target, content).await;
    };
    let (item, envelope) = {
        let _enqueue = runtime.enqueue.lock().await;
        let envelope = match payload::prepare(&runtime, target, content.clone()).await {
            Err(ChatChannelError::ConfigurationInvalid(reason))
                if reason == "TARGET_NOT_REGISTERED" =>
            {
                return direct(manager, target, content).await;
            }
            other => other?,
        };
        retire_progress(&runtime, &envelope).await?;
        let encoded =
            serde_json::to_string(&envelope).map_err(|_| payload::failure("INVALID_DELIVERY"))?;
        let item = match storage::enqueue_delivery(
            &runtime.db,
            (target.channel_id, &envelope.target_id),
            &encoded,
        )
        .await
        {
            Ok(item) => item,
            Err(_) => {
                payload::cleanup(&runtime, &envelope).await;
                return Err(payload::failure("DELIVERY_QUEUE_UNAVAILABLE"));
            }
        };
        (item, envelope)
    };
    worker::deliver(manager, &runtime, (item, envelope)).await
}

async fn direct(
    manager: &ChatChannelManager,
    target: &ChannelMessageTarget,
    content: Content,
) -> Result<SentMessageId, ChatChannelError> {
    let backend = manager.backend_for(target.channel_id).await?;
    match content {
        Content::Text(body) | Content::Progress(body, _) => {
            backend.send_rich_message_to(&body, target).await
        }
        Content::File(file) => backend.send_attachment_to(&file, target).await,
    }
}

async fn retire_progress(
    runtime: &Runtime,
    envelope: &payload::Envelope,
) -> Result<(), ChatChannelError> {
    let Some(task_id) = envelope.progress.as_deref() else {
        return Ok(());
    };
    let items = storage::deliveries_for_target(
        &runtime.db,
        (envelope.target.channel_id, &envelope.target_id),
    )
    .await
    .map_err(|_| payload::failure("DELIVERY_QUERY_FAILED"))?;
    for item in items {
        if serde_json::from_str::<payload::Envelope>(&item.content)
            .ok()
            .is_some_and(|entry| entry.progress.as_deref() == Some(task_id))
        {
            storage::remove_pending(&runtime.db, item.id)
                .await
                .map_err(|_| payload::failure("DELIVERY_MERGE_FAILED"))?;
        }
    }
    Ok(())
}
