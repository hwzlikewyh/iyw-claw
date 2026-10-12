mod scope;
mod state;
pub mod types;

use self::scope::{ChannelScope, PAGE_SIZE};
use self::types::*;
use super::manager::ChatChannelManager;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::LiveSessionSnapshot;
use crate::app_error::AppCommandError;
use crate::db::service::chat_channel_message_log_service;
use crate::models::chat_channel::ChatChannelMessageLogInfo;
use chrono::Utc;
use sea_orm::DatabaseConnection;

pub(crate) struct RuntimeContext<'a> {
    pub db: &'a DatabaseConnection,
    pub channels: &'a ChatChannelManager,
    pub connections: &'a ConnectionManager,
}

pub(crate) async fn list(
    context: RuntimeContext<'_>,
    query: ChannelRuntimeQuery,
) -> Result<ChannelConversationPage, AppCommandError> {
    let scope = ChannelScope::load(context.db, context.channels, query.channel_id).await?;
    let mut rows = scope.page(context.db, &query).await?;
    let has_more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let next_before = has_more
        .then(|| {
            rows.last().map(|row| ConversationCursor {
                id: row.id,
                updated_at: row.updated_at,
            })
        })
        .flatten();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let mut info = state::summary(row, query.channel_id);
        apply_live((&context, &scope), &mut info, false).await;
        items.push(info);
    }
    Ok(ChannelConversationPage {
        items,
        next_before,
        sampled_at: Utc::now().to_rfc3339(),
    })
}

pub(crate) async fn detail(
    context: RuntimeContext<'_>,
    query: ChannelConversationQuery,
) -> Result<ChannelConversationRuntime, AppCommandError> {
    let scope = ChannelScope::load(context.db, context.channels, query.channel_id).await?;
    let row = scope
        .conversation(context.db, query.conversation_id)
        .await?;
    let mut conversation = state::summary(row, query.channel_id);
    let snapshot = apply_live((&context, &scope), &mut conversation, true).await;
    let deliveries = delivery_logs(&context, &scope, query.conversation_id).await?;
    Ok(ChannelConversationRuntime {
        conversation,
        snapshot,
        deliveries,
        sampled_at: Utc::now().to_rfc3339(),
    })
}

async fn apply_live(
    runtime: (&RuntimeContext<'_>, &ChannelScope),
    info: &mut ChannelConversationInfo,
    include_snapshot: bool,
) -> Option<LiveSessionSnapshot> {
    let (context, scope) = runtime;
    let connection_id = match scope
        .routes
        .get(&info.conversation_id)
        .and_then(|route| route.connection_id.clone())
    {
        Some(id) => id,
        None => {
            context
                .connections
                .find_connection_by_conversation_id(info.conversation_id)
                .await?
        }
    };
    let state = context.connections.get_state(&connection_id).await?;
    let guard = state.read().await;
    if guard.conversation_id != Some(info.conversation_id) {
        if guard.conversation_id.is_none()
            && guard.status == crate::acp::types::ConnectionStatus::Connecting
        {
            info.phase = "preparing".into();
            info.connection_id = Some(connection_id);
        }
        return None;
    }
    state::apply(info, &guard);
    info.connection_id = Some(connection_id);
    include_snapshot.then(|| guard.to_snapshot())
}

async fn delivery_logs(
    context: &RuntimeContext<'_>,
    scope: &ChannelScope,
    conversation_id: i32,
) -> Result<Vec<ChatChannelMessageLogInfo>, AppCommandError> {
    let Some(trace_id) = scope
        .routes
        .get(&conversation_id)
        .and_then(|route| route.trace_id.as_deref())
    else {
        return Ok(Vec::new());
    };
    let rows = chat_channel_message_log_service::list_by_trace(context.db, trace_id, 20).await?;
    Ok(rows
        .into_iter()
        .filter(|row| row.direction == "outbound" && row.channel_id == scope.channel_id)
        .map(ChatChannelMessageLogInfo::from)
        .collect())
}
