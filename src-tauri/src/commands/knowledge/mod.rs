pub mod api;
pub mod library;
#[cfg(feature = "tauri-runtime")]
pub mod library_commands;
pub mod models;
mod results;
pub mod search;
pub mod selection;
mod tos_signing;
pub mod upload;
pub mod upload_models;
mod upload_store;
mod upload_worker;

#[cfg(feature = "tauri-runtime")]
use crate::app_error::AppCommandError;
#[cfg(feature = "tauri-runtime")]
use models::{BrowseFiles, FilePage, Folder, Selection, UpdateSelection};

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn knowledge_folders(
    category: u8,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<Vec<Folder>, AppCommandError> {
    api::KnowledgeClient::new(&db.conn)
        .await?
        .folders(category)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn knowledge_files(
    input: BrowseFiles,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<FilePage, AppCommandError> {
    api::KnowledgeClient::new(&db.conn)
        .await?
        .files(&input)
        .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn knowledge_selection_get(
    conversation_id: Option<i32>,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<Selection, AppCommandError> {
    selection::load(&db.conn, conversation_id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn knowledge_selection_set(
    input: UpdateSelection,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<Selection, AppCommandError> {
    selection::update(&db.conn, input).await
}
