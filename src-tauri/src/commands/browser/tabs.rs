use crate::browser::{
    AgentAccess, BrowserError, BrowserGenerations, BrowserSessionManager, BrowserStateSnapshot,
};

#[tauri::command]
pub async fn browser_create_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    url: String,
    access: AgentAccess,
    host_id: Option<String>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.create_browser_tab(url, access, host_id).await
}

#[tauri::command]
pub async fn browser_close_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.close_browser_tab(&tab_id).await
}

#[tauri::command]
pub async fn browser_navigate_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    url: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.navigate_browser_tab(&tab_id, url).await
}

#[tauri::command]
pub async fn browser_back(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.browser_back(&tab_id).await
}

#[tauri::command]
pub async fn browser_forward(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.browser_forward(&tab_id).await
}

#[tauri::command]
pub async fn browser_reload_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.reload_browser_tab(&tab_id).await
}

#[tauri::command]
pub async fn browser_resize_viewport(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    generations: BrowserGenerations,
    width: u32,
    height: u32,
    scale: f64,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .resize_browser_viewport(&tab_id, generations, width, height, scale)
        .await
}
