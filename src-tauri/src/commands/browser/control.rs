use crate::browser::{AgentAccess, BrowserSessionManager, BrowserStateSnapshot};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_set_user_held(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    held: bool,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager.set_user_held(&tab_id, held).await?;
        Ok(manager.snapshot().await)
    })
}

#[tauri::command(async)]
pub fn browser_set_tab_agent_access(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    access: AgentAccess,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.set_tab_agent_access(&tab_id, access).await })
}
