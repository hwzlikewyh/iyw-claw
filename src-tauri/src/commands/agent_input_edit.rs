use crate::acp::{error::AcpError, manager::ConnectionManager, AgentInputChange, AgentInputItem};
use crate::db::AppDatabase;

pub async fn revise_agent_input_core(
    db: &AppDatabase,
    manager: &ConnectionManager,
    connection_id: &str,
    conversation_id: i32,
    message_id: &str,
    change: AgentInputChange,
) -> Result<AgentInputItem, AcpError> {
    super::iyw_account::iyw_account_ensure_can_consume_core(&db.conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    if let AgentInputChange::Edit { payload } = &change {
        super::agent_input::validate_submit(conversation_id, message_id, payload)?;
        super::agent_input::require_payload_file_upload(payload).await?;
    }
    manager
        .revise_agent_input(db, connection_id, conversation_id, message_id, change)
        .await
        .map(|item| item.client_projection())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn revise_agent_input(
    db: tauri::State<'_, AppDatabase>,
    manager: tauri::State<'_, ConnectionManager>,
    connection_id: String,
    conversation_id: i32,
    message_id: String,
    change: AgentInputChange,
) -> Result<AgentInputItem, AcpError> {
    revise_agent_input_core(
        &db,
        &manager,
        &connection_id,
        conversation_id,
        &message_id,
        change,
    )
    .await
}
