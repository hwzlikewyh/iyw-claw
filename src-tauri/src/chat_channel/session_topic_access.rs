use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use super::session_bridge::SessionBridge;
use super::session_topic::CommandSessionRef;
use super::types::ChannelMessageTarget;
use crate::db::entities::chat_channel_thread_binding;
use crate::db::error::DbError;
use crate::db::service::conversation_binding_service::ConversationRoute;
use crate::db::service::{conversation_binding_service, thread_binding_service};

pub(super) struct CommandRouteAccess {
    pub has_session: bool,
    pub permission_pending: bool,
}

pub(super) async fn command_route_access(
    db: &DatabaseConnection,
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route_key: &str,
) -> CommandRouteAccess {
    let reference = super::session_topic::command_session_ref(
        db, bridge, channel_id, sender_id, target, route_key,
    )
    .await
    .ok()
    .flatten();
    let permission_pending = match reference.as_ref() {
        Some(reference) => bridge
            .lock()
            .await
            .get(&reference.connection_id)
            .and_then(|session| session.permission_pending.as_ref())
            .is_some(),
        None => false,
    };
    CommandRouteAccess {
        has_session: reference.is_some(),
        permission_pending,
    }
}

pub(super) async fn active_owned_binding_ref(
    db: &DatabaseConnection,
    bridge: &Arc<Mutex<SessionBridge>>,
    target: &ChannelMessageTarget,
    sender_id: &str,
) -> Result<Option<CommandSessionRef>, DbError> {
    let Some(binding) = thread_binding_service::get_owned_by_target(db, target, sender_id).await?
    else {
        return Ok(None);
    };
    Ok(active_binding_ref(bridge, target, &binding).await)
}

pub(super) async fn authorized_conversation_id(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
) -> Result<Option<i32>, DbError> {
    if target.channel_id != channel_id || target.is_telegram_general_topic() {
        return Ok(None);
    }
    if target.is_telegram_forum_topic() {
        return Ok(
            thread_binding_service::get_owned_by_target(db, target, sender_id)
                .await?
                .map(|binding| binding.conversation_id),
        );
    }
    Ok(
        conversation_binding_service::find_by_route(db, channel_id, &route.route_key)
            .await?
            .filter(|binding| binding.target_id == route.target_id)
            .map(|binding| binding.conversation_id),
    )
}

pub(super) async fn target_available_to_sender(
    db: &DatabaseConnection,
    target: &ChannelMessageTarget,
    sender_id: &str,
) -> Result<bool, DbError> {
    if !target.is_telegram_forum_topic() {
        return Ok(true);
    }
    Ok(thread_binding_service::get_by_target(db, target)
        .await?
        .is_none_or(|binding| binding.created_by_sender_id == sender_id))
}

pub(super) async fn active_binding_ref(
    bridge: &Arc<Mutex<SessionBridge>>,
    target: &ChannelMessageTarget,
    binding: &chat_channel_thread_binding::Model,
) -> Option<CommandSessionRef> {
    let guard = bridge.lock().await;
    let session = guard
        .find_by_target(target)
        .filter(|session| owns_binding(session, target, binding))
        .or_else(|| {
            binding
                .connection_id
                .as_deref()
                .and_then(|connection_id| guard.get(connection_id))
                .filter(|session| owns_binding(session, target, binding))
        })?;
    Some(CommandSessionRef {
        connection_id: session.connection_id.clone(),
        conversation_id: Some(session.conversation_id),
    })
}

fn owns_binding(
    session: &super::session_bridge::ActiveSession,
    target: &ChannelMessageTarget,
    binding: &chat_channel_thread_binding::Model,
) -> bool {
    binding_matches_target(binding, target)
        && binding.connection_id.as_deref() == Some(session.connection_id.as_str())
        && session.conversation_id == binding.conversation_id
        && session.sender_id == binding.created_by_sender_id
        && session.target.matches_thread(target)
}

fn binding_matches_target(
    binding: &chat_channel_thread_binding::Model,
    target: &ChannelMessageTarget,
) -> bool {
    binding.channel_id == target.channel_id
        && target.chat_id.as_deref() == Some(binding.chat_id.as_str())
        && target.thread_key.as_deref() == Some(binding.thread_key.as_str())
        && target.thread_kind.as_deref() == Some(binding.thread_kind.as_str())
}
