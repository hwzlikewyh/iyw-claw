use super::{library, upload, upload_models::*};
use crate::{app_error::AppCommandError, db::AppDatabase};

#[tauri::command]
pub async fn knowledge_library_folders(
    category: u8,
    db: tauri::State<'_, AppDatabase>,
) -> Result<library::LibraryFolders, AppCommandError> {
    library::folders(&db.conn, category).await
}

#[tauri::command]
pub async fn knowledge_library_files(
    input: library::LibraryQuery,
    db: tauri::State<'_, AppDatabase>,
) -> Result<library::LibraryFiles, AppCommandError> {
    library::files(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_library_refresh(
    input: library::RefreshFiles,
    db: tauri::State<'_, AppDatabase>,
) -> Result<library::RefreshedFiles, AppCommandError> {
    library::refresh(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_upload_begin(
    input: BeginUpload,
    db: tauri::State<'_, AppDatabase>,
) -> Result<UploadStatus, AppCommandError> {
    upload::begin(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_upload_append(
    input: AppendUpload,
    db: tauri::State<'_, AppDatabase>,
) -> Result<UploadStatus, AppCommandError> {
    upload::append(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_upload_start(
    input: UploadRef,
    db: tauri::State<'_, AppDatabase>,
) -> Result<UploadStatus, AppCommandError> {
    upload::start(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_upload_status(
    input: UploadRef,
    db: tauri::State<'_, AppDatabase>,
) -> Result<UploadStatus, AppCommandError> {
    upload::status(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_upload_abort(
    input: UploadRef,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    upload::abort(&db.conn, input).await
}
