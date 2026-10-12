use crate::app_error::AppCommandError;
pub use crate::chat_channel::runtime_view::types::*;
use crate::chat_channel::runtime_view::{self, RuntimeContext};

pub(crate) use runtime_view::{
    detail as get_chat_channel_conversation_runtime_core,
    list as list_chat_channel_conversations_core,
};

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn list_chat_channel_conversations(
    app: tauri::AppHandle,
    params: ChannelRuntimeQuery,
) -> Result<ChannelConversationPage, AppCommandError> {
    use tauri::Manager;
    let db = app.state::<crate::db::AppDatabase>();
    let channels = app.state::<crate::chat_channel::manager::ChatChannelManager>();
    let connections = app.state::<crate::acp::manager::ConnectionManager>();
    runtime_view::list(
        RuntimeContext {
            db: &db.conn,
            channels: &channels,
            connections: &connections,
        },
        params,
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_channel_conversation_runtime(
    app: tauri::AppHandle,
    params: ChannelConversationQuery,
) -> Result<ChannelConversationRuntime, AppCommandError> {
    use tauri::Manager;
    let db = app.state::<crate::db::AppDatabase>();
    let channels = app.state::<crate::chat_channel::manager::ChatChannelManager>();
    let connections = app.state::<crate::acp::manager::ConnectionManager>();
    runtime_view::detail(
        RuntimeContext {
            db: &db.conn,
            channels: &channels,
            connections: &connections,
        },
        params,
    )
    .await
}
