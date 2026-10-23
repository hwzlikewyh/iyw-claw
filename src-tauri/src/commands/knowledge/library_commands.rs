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
pub async fn knowledge_library_file_update(
    input: library::FileMutation,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    library::update_file(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_library_file_delete(
    input: library::FileMutation,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    library::delete_file(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_library_points(
    input: library::PointsQuery,
    db: tauri::State<'_, AppDatabase>,
) -> Result<serde_json::Value, AppCommandError> {
    library::points(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_library_point_create(
    input: library::PointMutation,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    library::create_point(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_library_point_update(
    input: library::PointMutation,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    library::update_point(&db.conn, input).await
}

#[tauri::command]
pub async fn knowledge_library_point_delete(
    input: library::PointMutation,
    db: tauri::State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    library::delete_point(&db.conn, input).await
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
