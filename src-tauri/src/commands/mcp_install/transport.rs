use super::{InstallInput, InstallTask};
use crate::app_error::AppCommandError;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_market_install_begin(
    input: InstallInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<InstallTask, AppCommandError> {
    let view = super::begin(&db.conn, input).await?;
    let id = view.id.clone();
    tokio::spawn(async move {
        use tauri::Manager;
        super::wait_for_completion(&id).await;
        super::super::mcp::refresh_tauri(&app, &app.state::<crate::db::AppDatabase>()).await;
    });
    Ok(view)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_market_install_list(
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<Vec<InstallTask>, AppCommandError> {
    super::list(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_market_install_status(
    task_id: String,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<InstallTask, AppCommandError> {
    super::status(&db.conn, &task_id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_market_install_cancel(
    task_id: String,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<InstallTask, AppCommandError> {
    super::cancel(&db.conn, &task_id).await
}
