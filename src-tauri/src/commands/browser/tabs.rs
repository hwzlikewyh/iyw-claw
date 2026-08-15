use crate::browser::{
    AgentAccess, BrowserGenerations, BrowserSessionManager, BrowserStateSnapshot,
};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_create_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    url: String,
    access: AgentAccess,
    host_id: Option<String>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.create_browser_tab(url, access, host_id).await })
}

#[tauri::command(async)]
pub fn browser_close_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.close_browser_tab(&tab_id).await })
}

#[tauri::command(async)]
pub fn browser_navigate_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    url: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.navigate_browser_tab(&tab_id, url).await })
}

#[tauri::command(async)]
pub fn browser_back(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.browser_back(&tab_id).await })
}

#[tauri::command(async)]
pub fn browser_forward(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.browser_forward(&tab_id).await })
}

#[tauri::command(async)]
pub fn browser_reload_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.reload_browser_tab(&tab_id).await })
}

#[tauri::command(async)]
pub fn browser_resize_viewport(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    generations: BrowserGenerations,
    width: u32,
    height: u32,
    scale: f64,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .resize_browser_viewport(&tab_id, generations, width, height, scale)
            .await
    })
}
