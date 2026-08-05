use std::path::{Path, PathBuf};

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use serde::Serialize;
use serde_json::Value;

use crate::db::entities::{conversation, task_artifact};
use crate::db::error::DbError;

const MAX_FILES: usize = 100;
const MAX_PATH_CHARS: usize = 4096;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactInfo {
    pub id: i32,
    pub conversation_id: i32,
    pub folder_id: i32,
    pub conversation_title: Option<String>,
    pub agent_type: String,
    pub path: String,
    pub display_name: String,
    pub created_at: String,
    pub last_checked_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactItemResult {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

struct ResolvedArtifact {
    source: String,
    path: PathBuf,
    display_name: String,
}

fn resolve_path(working_dir: &Path, source: &str) -> Result<(PathBuf, String), String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err("empty_path".into());
    }
    if trimmed.chars().count() > MAX_PATH_CHARS
        || trimmed.contains('\0')
        || is_windows_device_path(trimmed)
    {
        return Err("invalid_path".into());
    }
    let candidate = PathBuf::from(trimmed);
    let is_relative = !candidate.is_absolute();
    let joined = if is_relative {
        working_dir.join(candidate)
    } else {
        candidate
    };
    let metadata = std::fs::metadata(&joined).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => "missing".to_string(),
        _ => "inaccessible".to_string(),
    })?;
    if !metadata.is_file() {
        return Err("not_a_file".into());
    }
    let canonical = std::fs::canonicalize(&joined).map_err(|_| "inaccessible".to_string())?;
    if is_relative {
        let root = std::fs::canonicalize(working_dir).map_err(|_| "inaccessible".to_string())?;
        if !canonical.starts_with(root) {
            return Err("path_escape".into());
        }
    }
    let normalized = normalize_canonical_path(canonical);
    let display_name = normalized
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .to_string();
    Ok((normalized, display_name))
}

#[cfg(windows)]
fn is_windows_device_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\").to_ascii_lowercase();
    normalized.starts_with(r"\\.\") || normalized.starts_with(r"\\?\globalroot\")
}

#[cfg(not(windows))]
fn is_windows_device_path(_path: &str) -> bool {
    false
}

#[cfg(windows)]
fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    drop(value);
    path
}

#[cfg(not(windows))]
fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    path
}

fn resolve_files(
    working_dir: &Path,
    files: Vec<String>,
) -> (Vec<ResolvedArtifact>, Vec<ArtifactItemResult>) {
    let mut resolved = Vec::new();
    let mut rejected = Vec::new();
    for (index, source) in files.into_iter().enumerate() {
        let result = if index < MAX_FILES {
            resolve_path(working_dir, &source)
        } else {
            Err("too_many_files".into())
        };
        match result {
            Ok((path, display_name)) => resolved.push(ResolvedArtifact {
                source,
                path,
                display_name,
            }),
            Err(reason) => rejected.push(ArtifactItemResult {
                path: source,
                display_name: None,
                status: None,
                reason: Some(reason),
            }),
        }
    }
    (resolved, rejected)
}

async fn upsert_artifact<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    artifact: ResolvedArtifact,
) -> Result<ArtifactItemResult, DbError> {
    let path = artifact.path.to_string_lossy().to_string();
    let now = Utc::now();
    task_artifact::Entity::insert(task_artifact::ActiveModel {
        conversation_id: Set(conversation_id),
        path: Set(path.clone()),
        display_name: Set(artifact.display_name.clone()),
        source_path: Set(artifact.source),
        created_at: Set(now),
        last_checked_at: Set(now),
        status: Set("available".into()),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::columns([
            task_artifact::Column::ConversationId,
            task_artifact::Column::Path,
        ])
        .update_columns([
            task_artifact::Column::DisplayName,
            task_artifact::Column::LastCheckedAt,
            task_artifact::Column::Status,
        ])
        .to_owned(),
    )
    .exec(conn)
    .await?;
    Ok(ArtifactItemResult {
        path,
        display_name: Some(artifact.display_name),
        status: Some("available".into()),
        reason: None,
    })
}

pub async fn register_artifacts(
    conn: &DatabaseConnection,
    conversation_id: i32,
    working_dir: &Path,
    files: Vec<String>,
) -> Result<Value, DbError> {
    let (resolved, rejected) = resolve_files(working_dir, files);
    let mut accepted = Vec::new();
    let txn = conn.begin().await?;
    for artifact in resolved {
        accepted.push(upsert_artifact(&txn, conversation_id, artifact).await?);
    }
    txn.commit().await?;
    Ok(serde_json::json!({ "accepted": accepted, "rejected": rejected }))
}

pub async fn list_artifacts(
    conn: &DatabaseConnection,
    conversation_id: Option<i32>,
    folder_id: Option<i32>,
) -> Result<Vec<TaskArtifactInfo>, DbError> {
    let mut query = task_artifact::Entity::find()
        .inner_join(conversation::Entity)
        .order_by_desc(task_artifact::Column::CreatedAt);
    if let Some(id) = conversation_id {
        query = query.filter(task_artifact::Column::ConversationId.eq(id));
    }
    if let Some(id) = folder_id {
        query = query.filter(conversation::Column::FolderId.eq(id));
    }
    let rows = query.select_also(conversation::Entity).all(conn).await?;
    let mut results = Vec::with_capacity(rows.len());
    for (artifact, conversation) in rows {
        let Some(conversation) = conversation else {
            continue;
        };
        let status = current_status(Path::new(&artifact.path));
        let last_checked_at = if status != artifact.status {
            let now = Utc::now();
            let mut active: task_artifact::ActiveModel = artifact.clone().into();
            active.status = Set(status.clone());
            active.last_checked_at = Set(now);
            active.update(conn).await?;
            now
        } else {
            artifact.last_checked_at
        };
        results.push(TaskArtifactInfo {
            id: artifact.id,
            conversation_id: artifact.conversation_id,
            folder_id: conversation.folder_id,
            conversation_title: conversation.title,
            agent_type: conversation.agent_type,
            path: artifact.path,
            display_name: artifact.display_name,
            created_at: artifact.created_at.to_rfc3339(),
            last_checked_at: last_checked_at.to_rfc3339(),
            status,
        });
    }
    Ok(results)
}

fn current_status(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => "available",
        Ok(_) => "missing",
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => "inaccessible",
    }
    .to_string()
}
