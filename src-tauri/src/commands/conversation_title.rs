use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::chat_channel::manager::ChatChannelManager;
use crate::db::error::DbError;
use crate::db::service::conversation_service;
use crate::web::event_bridge::EventEmitter;

use super::conversations::{emit_conversation_upsert, sync_conversation_title_to_channels_core};

type TitleLock = AsyncMutex<()>;
type TitleLockRegistry = Mutex<HashMap<i32, Weak<TitleLock>>>;

static TITLE_LOCKS: OnceLock<TitleLockRegistry> = OnceLock::new();

pub(crate) struct ConversationTitleContext<'a> {
    pub conn: &'a sea_orm::DatabaseConnection,
    pub emitter: &'a EventEmitter,
    pub chat_channel_manager: &'a ChatChannelManager,
}

fn lock_registry() -> &'static TitleLockRegistry {
    TITLE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn lock_conversation(conversation_id: i32) -> OwnedMutexGuard<()> {
    let lock = {
        let mut registry = lock_registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = registry.get(&conversation_id).and_then(Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(TitleLock::new(()));
            registry.insert(conversation_id, Arc::downgrade(&lock));
            lock
        }
    };
    lock.lock_owned().await
}

async fn notify_title_changed(context: &ConversationTitleContext<'_>, conversation_id: i32) {
    emit_conversation_upsert(context.emitter, context.conn, conversation_id).await;
    sync_conversation_title_to_channels_core(
        context.conn,
        context.chat_channel_manager,
        conversation_id,
    )
    .await;
}

pub(crate) async fn update_manual(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    title: String,
) -> Result<(), DbError> {
    let _guard = lock_conversation(conversation_id).await;
    conversation_service::update_title(context.conn, conversation_id, title).await?;
    notify_title_changed(context, conversation_id).await;
    Ok(())
}

pub(crate) async fn refresh_auto(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    title: &str,
) -> Result<bool, DbError> {
    let _guard = lock_conversation(conversation_id).await;
    let changed =
        conversation_service::refresh_auto_title(context.conn, conversation_id, title.to_string())
            .await?;
    if changed {
        notify_title_changed(context, conversation_id).await;
    }
    Ok(changed)
}

pub(crate) async fn sync_channels(context: &ConversationTitleContext<'_>, conversation_id: i32) {
    let _guard = lock_conversation(conversation_id).await;
    sync_conversation_title_to_channels_core(
        context.conn,
        context.chat_channel_manager,
        conversation_id,
    )
    .await;
}

pub(crate) async fn notify_current(context: &ConversationTitleContext<'_>, conversation_id: i32) {
    let _guard = lock_conversation(conversation_id).await;
    notify_title_changed(context, conversation_id).await;
}
