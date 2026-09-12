pub mod management;
pub mod query;
pub(crate) mod source;

pub use query::list_artifacts;

use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serde::Serialize;
use serde_json::Value;

use crate::db::entities::{conversation, task_artifact};
use crate::db::error::DbError;
use source::{resolve_sources, ResolvedArtifact};

const CONVERSATION_TREE_BATCH_SIZE: usize = 500;
const MAX_CONVERSATION_ANCESTOR_DEPTH: usize = 256;
const MAX_CONVERSATION_SCOPE_IDS: usize = 5_000;
pub const DEFAULT_PAGE_SIZE: u64 = 50;
pub const MAX_PAGE_SIZE: u64 = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactInfo {
    pub id: i32,
    pub conversation_id: i32,
    pub message_id: Option<String>,
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
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactPage {
    pub items: Vec<TaskArtifactInfo>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactItemResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
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
    message_id: Option<&str>,
    turn_generation: Option<i64>,
    artifact: ResolvedArtifact,
) -> Result<ArtifactItemResult, DbError> {
    let path = artifact.path;
    let now = Utc::now();
    task_artifact::Entity::insert(task_artifact::ActiveModel {
        conversation_id: Set(conversation_id),
        message_id: Set(message_id.map(str::to_owned)),
        turn_generation: Set(turn_generation),
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
            task_artifact::Column::MessageId,
        ])
        .update_columns([
            task_artifact::Column::DisplayName,
            task_artifact::Column::MessageId,
            task_artifact::Column::Kind,
            task_artifact::Column::TurnGeneration,
            task_artifact::Column::LastCheckedAt,
            task_artifact::Column::Status,
        ])
        .to_owned(),
    )
    .exec(conn)
    .await?;
    Ok(ArtifactItemResult {
        id: Some(registered_artifact_id(conn, conversation_id, (&path, message_id)).await?),
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
    message_id: Option<&str>,
    turn_generation: Option<i64>,
    working_dir: &Path,
    files: Vec<String>,
) -> Result<Value, DbError> {
    let (resolved, rejected) = resolve_sources(working_dir, files);
    let rejected = rejected
        .into_iter()
        .map(|(path, reason)| ArtifactItemResult {
            id: None,
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
        accepted.push(
            upsert_artifact(&txn, conversation_id, message_id, turn_generation, artifact).await?,
        );
    }
    txn.commit().await?;
    Ok(serde_json::json!({ "accepted": accepted, "rejected": rejected }))
}

async fn registered_artifact_id<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
    reference: (&str, Option<&str>),
) -> Result<i32, DbError> {
    let query = task_artifact::Entity::find()
        .filter(task_artifact::Column::ConversationId.eq(conversation_id))
        .filter(task_artifact::Column::Path.eq(reference.0));
    let query = match reference.1 {
        Some(id) => query.filter(task_artifact::Column::MessageId.eq(id)),
        None => query.filter(task_artifact::Column::MessageId.is_null()),
    };
    query
        .order_by_desc(task_artifact::Column::Id)
        .one(conn)
        .await?
        .map(|artifact| artifact.id)
        .ok_or_else(|| DbError::NotFound("registered artifact".into()))
}

async fn conversation_scope_ids(
    conn: &DatabaseConnection,
    current_id: i32,
) -> Result<Vec<i32>, DbError> {
    let mut ids = conversation_ancestor_ids(conn, current_id).await?;
    ids.extend(conversation_descendant_ids(conn, current_id).await?);
    ids.sort_unstable();
    ids.dedup();
    if ids.len() > MAX_CONVERSATION_SCOPE_IDS {
        return Err(DbError::Validation(format!(
            "conversation scope exceeds {MAX_CONVERSATION_SCOPE_IDS} ids"
        )));
    }
    Ok(ids)
}

async fn conversation_ancestor_ids(
    conn: &DatabaseConnection,
    start_id: i32,
) -> Result<Vec<i32>, DbError> {
    let mut ids = Vec::new();
    let mut visited = HashSet::new();
    let mut current = Some(start_id);
    for _ in 0..MAX_CONVERSATION_ANCESTOR_DEPTH {
        let Some(id) = current else {
            return Ok(ids);
        };
        if !visited.insert(id) {
            return Err(DbError::Validation(
                "conversation parent cycle detected".to_string(),
            ));
        }
        ids.push(id);
        current = conversation::Entity::find_by_id(id)
            .one(conn)
            .await?
            .and_then(|conversation| conversation.parent_id);
    }
    Err(DbError::Validation(
        "conversation ancestor depth exceeds limit".to_string(),
    ))
}

async fn conversation_descendant_ids(
    conn: &DatabaseConnection,
    root_id: i32,
) -> Result<Vec<i32>, DbError> {
    let mut visited = HashSet::from([root_id]);
    let mut result = vec![root_id];
    let mut frontier = vec![root_id];
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for parent_ids in frontier.chunks(CONVERSATION_TREE_BATCH_SIZE) {
            let children = conversation::Entity::find()
                .filter(conversation::Column::ParentId.is_in(parent_ids.iter().copied()))
                .all(conn)
                .await?;
            for child in children {
                if visited.insert(child.id) {
                    next.push(child.id);
                    if result.len() + next.len() > MAX_CONVERSATION_SCOPE_IDS {
                        return Err(DbError::Validation(format!(
                            "conversation scope exceeds {MAX_CONVERSATION_SCOPE_IDS} ids"
                        )));
                    }
                }
            }
        }
        result.extend(next.iter().copied());
        frontier = next;
    }
    Ok(result)
}
