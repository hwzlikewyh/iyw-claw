use crate::browser::{
    BrowserError, BrowserGenerations, BrowserSessionManager, BrowserStateSnapshot,
};

#[tauri::command]
pub async fn browser_answer_dialog(
    manager: tauri::State<'_, BrowserSessionManager>,
    dialog_id: String,
    generations: BrowserGenerations,
    accept: bool,
    prompt_text: Option<String>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .answer_browser_dialog(&dialog_id, generations, accept, prompt_text)
        .await
}

#[tauri::command]
pub async fn browser_choose_files(
    manager: tauri::State<'_, BrowserSessionManager>,
    chooser_id: String,
    generations: BrowserGenerations,
    paths: Vec<String>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .choose_browser_files(&chooser_id, generations, paths)
        .await
}

#[tauri::command]
pub async fn browser_cancel_download(
    manager: tauri::State<'_, BrowserSessionManager>,
    download_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.cancel_browser_download(&download_id).await
}

#[tauri::command]
pub async fn browser_open_download(
    manager: tauri::State<'_, BrowserSessionManager>,
    download_id: String,
) -> Result<(), BrowserError> {
    manager.open_browser_download(&download_id).await
}

#[tauri::command]
pub async fn browser_reveal_download(
    manager: tauri::State<'_, BrowserSessionManager>,
    download_id: String,
) -> Result<(), BrowserError> {
    manager.reveal_browser_download(&download_id).await
}
