use crate::browser::{AgentAccess, BrowserError, BrowserSessionManager, BrowserStateSnapshot};

#[tauri::command]
pub async fn browser_set_user_held(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    held: bool,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.set_user_held(&tab_id, held).await?;
    Ok(manager.snapshot().await)
}

#[tauri::command]
pub async fn browser_set_tab_agent_access(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    access: AgentAccess,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.set_tab_agent_access(&tab_id, access).await
}
