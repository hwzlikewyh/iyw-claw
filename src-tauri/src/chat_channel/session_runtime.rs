use std::collections::BTreeMap;
use std::path::Path;

use sea_orm::DatabaseConnection;

use super::types::ChannelMessageTarget;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::PromptInputBlock;
use crate::db::service::folder_service;
use crate::db::AppDatabase;
use crate::models::agent::AgentType;
use crate::models::conversation::DbConversationSummary;
use crate::models::folder::FolderDetail;
use crate::web::event_bridge::EventEmitter;

pub(super) async fn build_runtime_env(
    db: &DatabaseConnection,
    agent_type: AgentType,
    session_id: Option<&str>,
    data_dir: &Path,
) -> Result<BTreeMap<String, String>, crate::acp::error::AcpError> {
    crate::commands::acp::build_session_runtime_env(
        &AppDatabase { conn: db.clone() },
        agent_type,
        session_id,
        data_dir,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn spawn_for_conversation(
    db: &DatabaseConnection,
    conversation: &DbConversationSummary,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    connection_manager: &ConnectionManager,
    emitter: &EventEmitter,
    data_dir: &Path,
) -> Result<(String, FolderDetail), String> {
    let folder = folder_service::get_folder_by_id(db, conversation.folder_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "folder not found".to_string())?;
    let runtime_env = build_runtime_env(
        db,
        conversation.agent_type,
        conversation.external_id.as_deref(),
        data_dir,
    )
    .await
    .map_err(|error| error.to_string())?;
    let connection_id = connection_manager
        .spawn_agent(
            conversation.agent_type,
            Some(folder.path.clone()),
            conversation.external_id.clone(),
            runtime_env,
            owner_label(channel_id, sender_id, target),
            emitter.clone(),
            None,
            BTreeMap::new(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection_id, folder))
}

pub(super) async fn send_prompt(
    connection_manager: &ConnectionManager,
    connection_id: &str,
    text: &str,
) -> Result<(), crate::acp::error::AcpError> {
    connection_manager
        .send_prompt(
            connection_id,
            vec![PromptInputBlock::Text {
                text: text.to_string(),
            }],
        )
        .await
}

pub(super) async fn send_prompt_linked(
    db: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    connection_id: &str,
    folder_id: i32,
    conversation_id: i32,
    text: &str,
) -> Result<(), crate::acp::error::AcpError> {
    connection_manager
        .send_prompt_linked(
            &AppDatabase { conn: db.clone() },
            connection_id,
            vec![PromptInputBlock::Text {
                text: text.to_string(),
            }],
            Some(folder_id),
            Some(conversation_id),
            None,
        )
        .await
        .map(|_| ())
}

pub(super) fn owner_label(
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
) -> String {
    match target
        .thread_key
        .as_deref()
        .filter(|_| target.is_telegram_forum_topic())
    {
        Some(thread_key) => {
            format!("chat_channel:{channel_id}:{sender_id}:thread:{thread_key}")
        }
        None => format!("chat_channel:{channel_id}:{sender_id}"),
    }
}
