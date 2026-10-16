#[cfg(feature = "tauri-runtime")]
use crate::acp::runtime_tasks::{control::TaskControl, RuntimeTaskSnapshot};
#[cfg(feature = "tauri-runtime")]
use crate::acp::{error::AcpError, manager::ConnectionManager};
#[cfg(feature = "tauri-runtime")]
use tauri::State;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_runtime_tasks(
    connection_id: Option<String>,
    conversation_id: Option<i32>,
    manager: State<'_, ConnectionManager>,
) -> Result<RuntimeTaskSnapshot, AcpError> {
    manager
        .runtime_tasks_for(connection_id.as_deref(), conversation_id)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_control_runtime_task(
    connection_id: String,
    request: TaskControl,
    manager: State<'_, ConnectionManager>,
) -> Result<RuntimeTaskSnapshot, AcpError> {
    manager.control_runtime_task(&connection_id, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn acp_runtime_task_detail(
    connection_id: String,
    task_id: String,
    manager: State<'_, ConnectionManager>,
) -> Result<crate::models::ConversationDetail, AcpError> {
    manager.runtime_task_detail(&connection_id, &task_id).await
}
