use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::chat_channel::runtime_view::RuntimeContext;
use crate::commands::chat_channel_runtime::{
    self as commands, ChannelConversationPage, ChannelConversationQuery,
    ChannelConversationRuntime, ChannelRuntimeQuery,
};
use axum::{extract::Extension, Json};
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct Params<T> {
    params: T,
}

pub async fn list_conversations(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<Params<ChannelRuntimeQuery>>,
) -> Result<Json<ChannelConversationPage>, AppCommandError> {
    Ok(Json(
        commands::list_chat_channel_conversations_core(
            RuntimeContext {
                db: &state.db.conn,
                channels: &state.chat_channel_manager,
                connections: &state.connection_manager,
            },
            request.params,
        )
        .await?,
    ))
}

pub async fn get_runtime(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<Params<ChannelConversationQuery>>,
) -> Result<Json<ChannelConversationRuntime>, AppCommandError> {
    Ok(Json(
        commands::get_chat_channel_conversation_runtime_core(
            RuntimeContext {
                db: &state.db.conn,
                channels: &state.chat_channel_manager,
                connections: &state.connection_manager,
            },
            request.params,
        )
        .await?,
    ))
}
