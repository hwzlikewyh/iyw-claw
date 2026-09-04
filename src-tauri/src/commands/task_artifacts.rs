use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::acp::delegation::listener::TaskArtifactAccess;
use crate::app_error::AppCommandError;
use crate::db::service::task_artifact_service::{self, TaskArtifactPage};
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

pub(crate) fn emit_task_artifacts_changed(emitter: &EventEmitter, conversation_id: i32) {
    emit_event(
        emitter,
        TASK_ARTIFACT_CHANGED_EVENT,
        TaskArtifactChange {
            conversation_id,
            change: "upserted",
        },
    );
}

#[async_trait]
impl TaskArtifactAccess for DbTaskArtifactAccess {
    async fn register_task_artifacts(
        &self,
        connection_id: &str,
        conversation_id: i32,
        message_id: Option<String>,
        turn_generation: Option<i64>,
        working_dir: &Path,
        files: Vec<String>,
    ) -> Value {
        let requested = files.len();
        let Some(message_id) = message_id else {
            tracing::warn!(
                connection_id,
                conversation_id,
                requested,
                "[task-artifacts] MCP registration rejected without an assistant message"
            );
            return artifact_message_unavailable(files);
        };
        let materialized = materialize_files(
            connection_id,
            conversation_id,
            turn_generation,
            &message_id,
            working_dir,
            files,
        )
        .await;
        match task_artifact_service::register_artifacts(
            &self.db.conn,
            conversation_id,
            Some(&message_id),
            turn_generation,
            working_dir,
            materialized.files,
        )
        .await
        {
            Ok(mut result) => {
                append_materialization_rejections(&mut result, materialized.rejected);
                if let Some(object) = result.as_object_mut() {
                    object.insert("message_id".into(), Value::String(message_id));
                }
                let accepted = result
                    .get("accepted")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                if accepted > 0 {
                    emit_task_artifacts_changed(&self.emitter, conversation_id);
                }
                tracing::info!(
                    conversation_id,
                    turn_generation,
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
                    turn_generation,
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

fn artifact_message_unavailable(files: Vec<String>) -> Value {
    json!({
        "accepted": [],
        "rejected": files.into_iter().map(|path| json!({
            "path": path,
            "reason": "assistant_message_unavailable"
        })).collect::<Vec<_>>(),
        "error": "assistant_message_unavailable"
    })
}

struct MaterializedArtifacts {
    files: Vec<String>,
    rejected: Vec<MaterializationRejection>,
}

struct MaterializationRejection {
    path: String,
    reason: &'static str,
}

async fn materialize_files(
    connection_id: &str,
    conversation_id: i32,
    turn_generation: Option<i64>,
    message_id: &str,
    working_dir: &Path,
    files: Vec<String>,
) -> MaterializedArtifacts {
    if !files.iter().any(|value| !is_url_source(value)) {
        return MaterializedArtifacts {
            files,
            rejected: Vec::new(),
        };
    }
    let Some(generation) = turn_generation.filter(|value| *value > 0) else {
        return reject_unmanaged_local_sources(files, "managed_directory_unavailable");
    };
    let Ok(directory) = crate::acp::task_artifact_delivery::ensure_managed_turn_directory(
        connection_id,
        conversation_id,
        generation,
    )
    .await
    else {
        return reject_unmanaged_local_sources(files, "managed_directory_unavailable");
    };
    let mut result = MaterializedArtifacts {
        files: Vec::with_capacity(files.len()),
        rejected: Vec::new(),
    };
    for (index, source) in files.into_iter().enumerate() {
        if is_url_source(&source) {
            result.files.push(source);
            continue;
        }
        if source.trim().is_empty() {
            result.rejected.push(MaterializationRejection {
                path: source,
                reason: "empty_source",
            });
            continue;
        }
        materialize_local_source(
            &mut result,
            &directory,
            working_dir,
            source,
            index,
            message_id,
        )
        .await;
    }
    result
}

fn reject_unmanaged_local_sources(
    files: Vec<String>,
    reason: &'static str,
) -> MaterializedArtifacts {
    let mut result = MaterializedArtifacts {
        files: Vec::with_capacity(files.len()),
        rejected: Vec::new(),
    };
    for source in files {
        if is_url_source(&source) {
            result.files.push(source);
        } else {
            result.rejected.push(MaterializationRejection {
                path: source,
                reason,
            });
        }
    }
    result
}

async fn materialize_local_source(
    result: &mut MaterializedArtifacts,
    directory: &Path,
    working_dir: &Path,
    source: String,
    index: usize,
    message_id: &str,
) {
    let source_path = match resolve_managed_source_path(working_dir, &source) {
        Ok(path) => path,
        Err(reason) => {
            result.rejected.push(MaterializationRejection {
                path: source,
                reason,
            });
            return;
        }
    };
    let Some(name) = source_path.file_name().and_then(|value| value.to_str()) else {
        result.rejected.push(MaterializationRejection {
            path: source,
            reason: "invalid_path",
        });
        return;
    };
    let target = directory.join(unique_name(name, index, message_id, &source_path));
    match copy_path(&source_path, &target).await {
        Ok(()) => result.files.push(target.to_string_lossy().into_owned()),
        Err(error) => {
            tracing::warn!(
                source = %source,
                error = %error,
                "[task-artifacts] MCP artifact materialization failed"
            );
            result.rejected.push(MaterializationRejection {
                path: source,
                reason: "materialize_failed",
            });
        }
    }
}

fn is_url_source(value: &str) -> bool {
    value.split_once("://").is_some()
}

fn resolve_managed_source_path(working_dir: &Path, source: &str) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(source);
    let relative = !path.is_absolute();
    let candidate = if relative {
        working_dir.join(path)
    } else {
        path
    };
    let canonical = std::fs::canonicalize(candidate).map_err(map_source_path_error)?;
    if relative {
        let root = std::fs::canonicalize(working_dir).map_err(map_source_path_error)?;
        if !canonical.starts_with(root) {
            return Err("path_escape");
        }
    }
    Ok(canonical)
}

fn map_source_path_error(error: std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "missing",
        _ => "inaccessible",
    }
}

fn append_materialization_rejections(result: &mut Value, rejected: Vec<MaterializationRejection>) {
    if rejected.is_empty() {
        return;
    }
    let Some(object) = result.as_object_mut() else {
        return;
    };
    let entries = object
        .entry("rejected")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(entries) = entries.as_array_mut() else {
        return;
    };
    entries.extend(
        rejected
            .into_iter()
            .map(|entry| json!({"path": entry.path, "reason": entry.reason})),
    );
}

fn unique_name(name: &str, index: usize, message_id: &str, source: &Path) -> String {
    let prefix = message_id
        .rsplit('-')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("artifact");
    let source = source.to_string_lossy();
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    format!("{prefix}-{index}-{}-{name}", &digest[..16])
}

async fn copy_path(source: &Path, target: &Path) -> std::io::Result<()> {
    let source = source.to_owned();
    let target = target.to_owned();
    tokio::task::spawn_blocking(move || copy_path_blocking(&source, &target))
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string())))
}

fn copy_path_blocking(source: &Path, target: &Path) -> std::io::Result<()> {
    let metadata = std::fs::metadata(source)?;
    if metadata.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_path_blocking(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
    }
    Ok(())
}

pub async fn list_task_artifacts_core(
    conn: &DatabaseConnection,
    conversation_id: Option<i32>,
    message_id: Option<String>,
    folder_id: Option<i32>,
    latest_turn_only: bool,
    search: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
) -> Result<TaskArtifactPage, AppCommandError> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size
        .unwrap_or(task_artifact_service::DEFAULT_PAGE_SIZE)
        .clamp(1, task_artifact_service::MAX_PAGE_SIZE);
    let started = Instant::now();
    if conversation_id.is_some_and(|id| id <= 0)
        || folder_id.is_some_and(|id| id <= 0)
        || (latest_turn_only && (conversation_id.is_none() || folder_id.is_some()))
    {
        tracing::warn!(
            conversation_id,
            folder_id,
            latest_turn_only,
            "[task-artifacts] invalid list filters"
        );
        return Err(AppCommandError::invalid_input(
            "Invalid artifact filter combination",
        ));
    }
    match task_artifact_service::list_artifacts(
        conn,
        conversation_id,
        message_id.as_deref(),
        folder_id,
        latest_turn_only,
        search.as_deref(),
        page,
        page_size,
    )
    .await
    {
        Ok(result) => {
            tracing::info!(
                conversation_id,
                folder_id,
                latest_turn_only,
                search_chars = search
                    .as_deref()
                    .map(|value| value.chars().count())
                    .unwrap_or(0),
                page,
                page_size,
                count = result.items.len(),
                total = result.total,
                elapsed_ms = started.elapsed().as_millis(),
                "[task-artifacts] list completed"
            );
            Ok(result)
        }
        Err(error) => {
            tracing::error!(
                conversation_id,
                folder_id,
                latest_turn_only,
                elapsed_ms = started.elapsed().as_millis(),
                error = %error,
                "[task-artifacts] list failed"
            );
            Err(AppCommandError::from(error))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskArtifactsParams {
    pub conversation_id: Option<i32>,
    pub message_id: Option<String>,
    pub folder_id: Option<i32>,
    pub latest_turn_only: Option<bool>,
    pub search: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn list_task_artifacts(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, AppDatabase>,
    conversation_id: Option<i32>,
    message_id: Option<String>,
    folder_id: Option<i32>,
    latest_turn_only: Option<bool>,
    search: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
) -> Result<TaskArtifactPage, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        list_task_artifacts_core(
            &db.conn,
            conversation_id,
            message_id,
            folder_id,
            latest_turn_only.unwrap_or(false),
            search,
            page,
            page_size,
        )
        .await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = (
            conversation_id,
            message_id,
            folder_id,
            latest_turn_only,
            search,
            page,
            page_size,
        );
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
