use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;

use super::manager::ChatChannelManager;
use super::types::{ChannelConnectionStatus, ChannelRuntimeEvent, ChannelRuntimeStatus};
use crate::db::service::chat_channel_service;

pub fn spawn_runtime_status_listener(
    receiver: Option<mpsc::Receiver<ChannelRuntimeEvent>>,
    manager: ChatChannelManager,
    db: DatabaseConnection,
) {
    let Some(mut receiver) = receiver else {
        tracing::warn!("[ChatChannel] runtime status receiver already taken");
        return;
    };
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            apply_runtime_event(&db, &manager, event).await;
        }
        tracing::warn!("[ChatChannel] runtime status listener stopped");
    });
}

async fn apply_runtime_event(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    event: ChannelRuntimeEvent,
) {
    let (channel_id, generation, expected, status, error, error_at, connected_at) = match event {
        ChannelRuntimeEvent::Connected {
            channel_id,
            generation,
        } => (
            channel_id,
            generation,
            ChannelConnectionStatus::Connected,
            ChannelRuntimeStatus::Connected,
            Some(None),
            Some(None),
            Some(Some(chrono::Utc::now())),
        ),
        ChannelRuntimeEvent::Error {
            channel_id,
            generation,
            error,
        } => (
            channel_id,
            generation,
            ChannelConnectionStatus::Error,
            ChannelRuntimeStatus::Error,
            Some(Some(error)),
            Some(Some(chrono::Utc::now())),
            None,
        ),
    };
    if manager
        .connection_status_for_generation(channel_id, generation)
        .await
        != Some(expected)
    {
        tracing::debug!(
            channel_id,
            ?expected,
            "[ChatChannel] ignored stale runtime event"
        );
        return;
    }
    if let Err(db_error) = chat_channel_service::update_runtime(
        db,
        channel_id,
        Some(status.as_str().to_string()),
        error,
        error_at,
        connected_at,
    )
    .await
    {
        tracing::error!(channel_id, error = %db_error, "[ChatChannel] runtime state persistence failed");
    }
    manager
        .emit_channel_status(channel_id, status.as_str())
        .await;
}
