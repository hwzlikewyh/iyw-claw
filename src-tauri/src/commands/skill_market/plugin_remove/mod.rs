mod apply;
mod catalog;
mod cleanup;
mod commit;
mod fingerprint;
mod inspect;
mod journal;
mod recover;
mod types;
use crate::app_error::AppCommandError;
pub use apply::{apply, retry_cleanup, status};
pub use inspect::inspect;
pub use recover::recover;
pub use types::{ApplyInput, InspectInput, RemovalReview};

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_remove_recover(
    input: InspectInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<RemovalReview, AppCommandError> {
    recover(
        &db.conn,
        input,
        crate::web::event_bridge::EventEmitter::Tauri(app),
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_remove_inspect(
    input: InspectInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<RemovalReview, AppCommandError> {
    inspect(&db.conn, input).await
}
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_remove_apply(
    input: ApplyInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<RemovalReview, AppCommandError> {
    apply(
        &db.conn,
        input,
        crate::web::event_bridge::EventEmitter::Tauri(app),
    )
    .await
}
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_remove_status(
    input: InspectInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<Option<RemovalReview>, AppCommandError> {
    status(&db.conn, input).await
}
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_remove_cleanup(
    input: InspectInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<RemovalReview, AppCommandError> {
    retry_cleanup(&db.conn, input).await
}
