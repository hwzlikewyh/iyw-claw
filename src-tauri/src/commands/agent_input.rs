use sea_orm::DatabaseConnection;

use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::{AgentInputItem, AgentInputPayload};
use crate::app_error::AppCommandError;
use crate::db::service::agent_input_outbox_service;
use crate::db::AppDatabase;

const MAX_MESSAGE_ID_CHARS: usize = 160;

fn validate_submit(
    conversation_id: i32,
    message_id: &str,
    payload: &AgentInputPayload,
) -> Result<(), AcpError> {
    if conversation_id <= 0 {
        return Err(AcpError::protocol(
            "conversation id must be positive".into(),
        ));
    }
    let id = message_id.trim();
    if id.is_empty() || id.chars().count() > MAX_MESSAGE_ID_CHARS {
        return Err(AcpError::protocol("invalid agent input message id".into()));
    }
    if payload.blocks.is_empty() || !payload_has_content(payload) {
        return Err(AcpError::protocol("agent input cannot be empty".into()));
    }
    Ok(())
}

fn payload_has_content(payload: &AgentInputPayload) -> bool {
    payload.blocks.iter().any(|block| match block {
        crate::acp::PromptInputBlock::Text { text } => !text.trim().is_empty(),
        crate::acp::PromptInputBlock::Image {
            data, mime_type, ..
        } => !data.trim().is_empty() && !mime_type.trim().is_empty(),
        crate::acp::PromptInputBlock::Resource { uri, .. } => !uri.trim().is_empty(),
        crate::acp::PromptInputBlock::ResourceLink { uri, name, .. } => {
            !uri.trim().is_empty() && !name.trim().is_empty()
        }
    })
}

pub async fn submit_agent_input_core(
    db: &AppDatabase,
    manager: &ConnectionManager,
    connection_id: String,
    conversation_id: i32,
    message_id: String,
    payload: AgentInputPayload,
) -> Result<AgentInputItem, AcpError> {
    validate_submit(conversation_id, &message_id, &payload)?;
    manager
        .submit_agent_input(db, &connection_id, conversation_id, message_id, payload)
        .await
}

pub async fn list_agent_inputs_core(
    db: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<AgentInputItem>, AppCommandError> {
    if conversation_id <= 0 {
        return Err(AppCommandError::invalid_input(
            "Conversation id must be positive",
        ));
    }
    agent_input_outbox_service::list_visible(db, conversation_id)
        .await
        .map_err(AppCommandError::from)
}

pub async fn delete_agent_input_core(
    db: &AppDatabase,
    manager: &ConnectionManager,
    connection_id: String,
    conversation_id: i32,
    message_id: String,
) -> Result<AgentInputItem, AcpError> {
    manager
        .delete_agent_input(db, &connection_id, conversation_id, &message_id)
        .await
}

pub async fn retry_agent_input_core(
    db: &AppDatabase,
    manager: &ConnectionManager,
    connection_id: String,
    conversation_id: i32,
    message_id: String,
) -> Result<AgentInputItem, AcpError> {
    manager
        .retry_agent_input(db, &connection_id, conversation_id, &message_id)
        .await
}

pub async fn resume_agent_inputs_core(
    db: &AppDatabase,
    manager: &ConnectionManager,
    connection_id: String,
    conversation_id: i32,
) -> Result<(), AcpError> {
    manager
        .resume_agent_input_connection(db, &connection_id, conversation_id)
        .await
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn submit_agent_input(
    connection_id: String,
    conversation_id: i32,
    message_id: String,
    payload: AgentInputPayload,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
    #[cfg(feature = "tauri-runtime")] manager: tauri::State<'_, ConnectionManager>,
) -> Result<AgentInputItem, AcpError> {
    #[cfg(feature = "tauri-runtime")]
    {
        submit_agent_input_core(
            &db,
            &manager,
            connection_id,
            conversation_id,
            message_id,
            payload,
        )
        .await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = (connection_id, conversation_id, message_id, payload);
        Err(AcpError::protocol("tauri-only command".into()))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn list_agent_inputs(
    conversation_id: i32,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
) -> Result<Vec<AgentInputItem>, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        list_agent_inputs_core(&db.conn, conversation_id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = conversation_id;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn delete_agent_input(
    connection_id: String,
    conversation_id: i32,
    message_id: String,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
    #[cfg(feature = "tauri-runtime")] manager: tauri::State<'_, ConnectionManager>,
) -> Result<AgentInputItem, AcpError> {
    #[cfg(feature = "tauri-runtime")]
    {
        delete_agent_input_core(&db, &manager, connection_id, conversation_id, message_id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = (connection_id, conversation_id, message_id);
        Err(AcpError::protocol("tauri-only command".into()))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn retry_agent_input(
    connection_id: String,
    conversation_id: i32,
    message_id: String,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
    #[cfg(feature = "tauri-runtime")] manager: tauri::State<'_, ConnectionManager>,
) -> Result<AgentInputItem, AcpError> {
    #[cfg(feature = "tauri-runtime")]
    {
        retry_agent_input_core(&db, &manager, connection_id, conversation_id, message_id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = (connection_id, conversation_id, message_id);
        Err(AcpError::protocol("tauri-only command".into()))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn resume_agent_inputs(
    connection_id: String,
    conversation_id: i32,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
    #[cfg(feature = "tauri-runtime")] manager: tauri::State<'_, ConnectionManager>,
) -> Result<(), AcpError> {
    #[cfg(feature = "tauri-runtime")]
    {
        resume_agent_inputs_core(&db, &manager, connection_id, conversation_id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = (connection_id, conversation_id);
        Err(AcpError::protocol("tauri-only command".into()))
    }
}
