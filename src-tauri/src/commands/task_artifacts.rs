use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::acp::delegation::listener::TaskArtifactAccess;
use crate::app_error::AppCommandError;
use crate::db::service::task_artifact_service::{self, TaskArtifactInfo};
use crate::db::AppDatabase;
use crate::web::event_bridge::{emit_event, EventEmitter, TASK_ARTIFACT_CHANGED_EVENT};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactChange {
    pub conversation_id: i32,
    pub change: &'static str,
}

pub struct DbTaskArtifactAccess {
    db: Arc<AppDatabase>,
    emitter: EventEmitter,
}

impl DbTaskArtifactAccess {
    pub fn new(db: Arc<AppDatabase>, emitter: EventEmitter) -> Self {
        Self { db, emitter }
    }
}

#[async_trait]
impl TaskArtifactAccess for DbTaskArtifactAccess {
    async fn register_task_artifacts(
        &self,
        conversation_id: i32,
        working_dir: &Path,
        files: Vec<String>,
    ) -> Value {
        let requested = files.len();
        match task_artifact_service::register_artifacts(
            &self.db.conn,
            conversation_id,
            working_dir,
            files,
        )
        .await
        {
            Ok(result) => {
                let accepted = result
                    .get("accepted")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                if accepted > 0 {
                    emit_event(
                        &self.emitter,
                        TASK_ARTIFACT_CHANGED_EVENT,
                        TaskArtifactChange {
                            conversation_id,
                            change: "upserted",
                        },
                    );
                }
                tracing::info!(
                    conversation_id,
                    requested,
                    accepted,
                    rejected = requested.saturating_sub(accepted),
                    "[task-artifacts] MCP registration processed"
                );
                result
            }
            Err(error) => {
                tracing::error!(
                    conversation_id,
                    requested,
                    error = %error,
                    "[task-artifacts] MCP registration failed"
                );
                serde_json::json!({
                    "accepted": [],
                    "rejected": [],
                    "error": "persistence_failed"
                })
            }
        }
    }
}

pub async fn list_task_artifacts_core(
    conn: &DatabaseConnection,
    conversation_id: Option<i32>,
    folder_id: Option<i32>,
) -> Result<Vec<TaskArtifactInfo>, AppCommandError> {
    if conversation_id.is_some_and(|id| id <= 0) || folder_id.is_some_and(|id| id <= 0) {
        return Err(AppCommandError::invalid_input(
            "Artifact filter IDs must be positive",
        ));
    }
    task_artifact_service::list_artifacts(conn, conversation_id, folder_id)
        .await
        .map_err(AppCommandError::from)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskArtifactsParams {
    pub conversation_id: Option<i32>,
    pub folder_id: Option<i32>,
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn list_task_artifacts(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
    conversation_id: Option<i32>,
    folder_id: Option<i32>,
) -> Result<Vec<TaskArtifactInfo>, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        list_task_artifacts_core(&db.conn, conversation_id, folder_id).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = (conversation_id, folder_id);
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn copy_file_to_clipboard(
    #[cfg(feature = "tauri-runtime")] window: tauri::WebviewWindow,
    path: String,
) -> Result<(), AppCommandError> {
    #[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
    {
        let owner = window.hwnd().map_err(|error| {
            tracing::error!(
                action = "copy_file",
                error = %error,
                "[task-artifacts] artifact window handle unavailable"
            );
            AppCommandError::window("Could not access the artifact window", error.to_string())
        })?;
        let owner = owner.0 as isize;
        let result = tokio::task::spawn_blocking(move || copy_artifact_path_blocking(path, owner))
            .await
            .map_err(|error| {
                tracing::error!(
                    action = "copy_file",
                    error = %error,
                    "[task-artifacts] clipboard worker failed"
                );
                AppCommandError::task_execution_failed("File clipboard worker failed")
                    .with_detail(error.to_string())
            })?;
        log_copy_artifact_result(&result);
        result
    }
    #[cfg(not(all(feature = "tauri-runtime", target_os = "windows")))]
    {
        #[cfg(feature = "tauri-runtime")]
        let _ = window;
        let _ = path;
        Err(AppCommandError::configuration_invalid(
            "File clipboard is only available on Windows desktop",
        ))
    }
}

#[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
fn copy_artifact_path_blocking(path: String, owner: isize) -> Result<(), AppCommandError> {
    let validated = validate_artifact_path(&path)?;
    crate::windows_file_clipboard::copy_file(Path::new(validated), owner).map_err(|error| {
        AppCommandError::io_error("Could not copy artifact").with_detail(error.to_string())
    })
}

#[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
fn log_copy_artifact_result(result: &Result<(), AppCommandError>) {
    match result {
        Ok(()) => tracing::info!(
            action = "copy_artifact",
            "[task-artifacts] artifact copied to clipboard"
        ),
        Err(error) => tracing::error!(
            action = "copy_artifact",
            code = ?error.code,
            reason = %error.message,
            "[task-artifacts] artifact clipboard action failed"
        ),
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn open_path_with_picker(path: String) -> Result<(), AppCommandError> {
    #[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
    {
        let normalized = validate_artifact_file(&path)?;
        crate::process::std_command("rundll32.exe")
            .arg("shell32.dll,OpenAs_RunDLL")
            .arg(normalized)
            .spawn()
            .map_err(|error| {
                AppCommandError::external_command(
                    "Could not open the application picker",
                    error.to_string(),
                )
            })?;
        Ok(())
    }
    #[cfg(not(all(feature = "tauri-runtime", target_os = "windows")))]
    {
        let _ = path;
        Err(AppCommandError::configuration_invalid(
            "Application picker is only available on Windows desktop",
        ))
    }
}

fn validate_artifact_path(path: &str) -> Result<&str, AppCommandError> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(AppCommandError::invalid_input("Invalid artifact path"));
    }
    let metadata = std::fs::metadata(path).map_err(|error| {
        AppCommandError::invalid_input("Artifact is unavailable").with_detail(error.to_string())
    })?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(AppCommandError::invalid_input(
            "Artifact path is not a file or directory",
        ));
    }
    Ok(path)
}

fn validate_artifact_file(path: &str) -> Result<&str, AppCommandError> {
    let normalized = validate_artifact_path(path)?;
    if !Path::new(normalized).is_file() {
        return Err(AppCommandError::invalid_input(
            "Artifact path is not a file",
        ));
    }
    Ok(normalized)
}
