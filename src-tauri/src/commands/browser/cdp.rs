use crate::browser::{BrowserGenerations, BrowserSessionManager, BrowserStateSnapshot};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_answer_dialog(
    manager: tauri::State<'_, BrowserSessionManager>,
    dialog_id: String,
    generations: BrowserGenerations,
    accept: bool,
    prompt_text: Option<String>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .answer_browser_dialog(&dialog_id, generations, accept, prompt_text)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_choose_files(
    manager: tauri::State<'_, BrowserSessionManager>,
    chooser_id: String,
    generations: BrowserGenerations,
    paths: Vec<String>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .choose_browser_files(&chooser_id, generations, paths)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_cancel_download(
    manager: tauri::State<'_, BrowserSessionManager>,
    download_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.cancel_browser_download(&download_id).await })
}

#[tauri::command(async)]
pub fn browser_open_download(
    manager: tauri::State<'_, BrowserSessionManager>,
    download_id: String,
) -> BrowserCommandFuture<()> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.open_browser_download(&download_id).await })
}

#[tauri::command(async)]
pub fn browser_reveal_download(
    manager: tauri::State<'_, BrowserSessionManager>,
    download_id: String,
) -> BrowserCommandFuture<()> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.reveal_browser_download(&download_id).await })
}
