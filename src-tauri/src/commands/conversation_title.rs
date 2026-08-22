use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::chat_channel::manager::ChatChannelManager;
use crate::db::entities::conversation::ConversationTitleSource;
use crate::db::error::DbError;
use crate::db::service::conversation_title_service;
use crate::web::event_bridge::EventEmitter;

use super::conversations::{
    notify_conversation_title_updates, spawn_conversation_title_channel_sync,
};

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
    drop(notify_conversation_title_updates(context, vec![conversation_id]).await);
}

pub(crate) async fn update_manual(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    title: String,
) -> Result<(), DbError> {
    let guard = lock_conversation(conversation_id).await;
    conversation_title_service::update_manual(context.conn, conversation_id, title).await?;
    drop(guard);
    notify_title_changed(context, conversation_id).await;
    Ok(())
}

pub(crate) async fn refresh_auto(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    title: &str,
) -> Result<bool, DbError> {
    refresh_with_source(
        context,
        TitleRefresh {
            conversation_id,
            title,
            source: ConversationTitleSource::Agent,
        },
    )
    .await
}

pub(crate) async fn refresh_fallback(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    title: &str,
) -> Result<bool, DbError> {
    refresh_with_source(
        context,
        TitleRefresh {
            conversation_id,
            title,
            source: ConversationTitleSource::UserFallback,
        },
    )
    .await
}

pub(crate) async fn refresh_summary(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    title: &str,
) -> Result<bool, DbError> {
    refresh_with_source(
        context,
        TitleRefresh {
            conversation_id,
            title,
            source: ConversationTitleSource::CodexSummary,
        },
    )
    .await
}

struct TitleRefresh<'a> {
    conversation_id: i32,
    title: &'a str,
    source: ConversationTitleSource,
}

async fn refresh_with_source(
    context: &ConversationTitleContext<'_>,
    refresh: TitleRefresh<'_>,
) -> Result<bool, DbError> {
    let guard = lock_conversation(refresh.conversation_id).await;
    let update = conversation_title_service::AutoTitleUpdate {
        conversation_id: refresh.conversation_id,
        title: refresh.title,
        source: refresh.source,
    };
    let changed = conversation_title_service::refresh(context.conn, update).await?;
    drop(guard);
    if changed {
        notify_title_changed(context, refresh.conversation_id).await;
    }
    Ok(changed)
}

pub(crate) async fn sync_channels(context: &ConversationTitleContext<'_>, conversation_id: i32) {
    spawn_conversation_title_channel_sync(context, conversation_id);
}

pub(crate) async fn notify_current(context: &ConversationTitleContext<'_>, conversation_id: i32) {
    notify_title_changed(context, conversation_id).await;
}
