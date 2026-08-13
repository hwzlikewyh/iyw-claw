mod source;

use std::path::Path;

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
use source::{current_artifact_state, resolve_sources, ResolvedArtifact};

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
    pub kind: String,
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
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
async fn upsert_artifact<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    artifact: ResolvedArtifact,
) -> Result<ArtifactItemResult, DbError> {
    let path = artifact.path;
    let now = Utc::now();
    task_artifact::Entity::insert(task_artifact::ActiveModel {
        conversation_id: Set(conversation_id),
        path: Set(path.clone()),
        display_name: Set(artifact.display_name.clone()),
        kind: Set(artifact.kind.clone()),
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
            task_artifact::Column::Kind,
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
        kind: Some(artifact.kind),
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
    let (resolved, rejected) = resolve_sources(working_dir, files);
    let rejected = rejected
        .into_iter()
        .map(|(path, reason)| ArtifactItemResult {
            path,
            display_name: None,
            kind: None,
            status: None,
            reason: Some(reason),
        })
        .collect::<Vec<_>>();
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
        let current = current_artifact_state(&artifact.path, &artifact.kind);
        let last_checked_at = persist_current_state(conn, &artifact, &current).await;
        results.push(TaskArtifactInfo {
            id: artifact.id,
            conversation_id: artifact.conversation_id,
            folder_id: conversation.folder_id,
            conversation_title: conversation.title,
            agent_type: conversation.agent_type,
            path: artifact.path,
            display_name: artifact.display_name,
            kind: current.kind,
            created_at: artifact.created_at.to_rfc3339(),
            last_checked_at: last_checked_at.to_rfc3339(),
            status: current.status,
        });
    }
    Ok(results)
}

async fn persist_current_state(
    conn: &DatabaseConnection,
    artifact: &task_artifact::Model,
    current: &source::CurrentArtifactState,
) -> chrono::DateTime<Utc> {
    if current.status == artifact.status && current.kind == artifact.kind {
        return artifact.last_checked_at;
    }
    let now = Utc::now();
    let mut active: task_artifact::ActiveModel = artifact.clone().into();
    active.status = Set(current.status.clone());
    active.kind = Set(current.kind.clone());
    active.last_checked_at = Set(now);
    if let Err(error) = active.update(conn).await {
        tracing::warn!(
            artifact_id = artifact.id,
            conversation_id = artifact.conversation_id,
            status = current.status,
            kind = current.kind,
            error = %error,
            "[task-artifacts] state cache update failed"
        );
        return artifact.last_checked_at;
    }
    now
}
