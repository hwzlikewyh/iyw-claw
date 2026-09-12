use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Condition, Expr, ExprTrait, Func, LikeExpr};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Select, Set,
};

use super::source::{current_artifact_state, CurrentArtifactState};
use super::{conversation_scope_ids, TaskArtifactInfo, TaskArtifactPage, MAX_PAGE_SIZE};
use crate::db::entities::{conversation, task_artifact};
use crate::db::error::DbError;

pub struct ArtifactQuery<'a> {
    pub conversation_id: Option<i32>,
    pub message_id: Option<&'a str>,
    pub folder_id: Option<i32>,
    pub latest_turn_only: bool,
    pub include_related_conversations: bool,
    pub search: Option<&'a str>,
    pub page: u64,
    pub page_size: u64,
}

pub async fn list_artifacts(
    conn: &DatabaseConnection,
    conversation_id: Option<i32>,
    message_id: Option<&str>,
    folder_id: Option<i32>,
    latest_turn_only: bool,
    search: Option<&str>,
    page: u64,
    page_size: u64,
) -> Result<TaskArtifactPage, DbError> {
    query_artifacts(
        conn,
        ArtifactQuery {
            conversation_id,
            message_id,
            folder_id,
            latest_turn_only,
            include_related_conversations: true,
            search,
            page,
            page_size,
        },
    )
    .await
}

pub async fn query_artifacts(
    conn: &DatabaseConnection,
    filters: ArtifactQuery<'_>,
) -> Result<TaskArtifactPage, DbError> {
    let mut query = task_artifact::Entity::find()
        .inner_join(conversation::Entity)
        .order_by_desc(task_artifact::Column::CreatedAt)
        .order_by_desc(task_artifact::Column::Id)
        .filter(conversation_filter(conn, &filters).await?);
    if let Some(id) = filters.message_id.filter(|value| !value.trim().is_empty()) {
        query = query.filter(task_artifact::Column::MessageId.eq(id));
    }
    if let Some(id) = filters.folder_id {
        query = query.filter(conversation::Column::FolderId.eq(id));
    }
    if let Some(search) = filters
        .search
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query = query.filter(search_condition(search));
    }
    fetch_page(conn, query, (filters.page, filters.page_size)).await
}

async fn conversation_filter(
    conn: &DatabaseConnection,
    filters: &ArtifactQuery<'_>,
) -> Result<Condition, DbError> {
    let mut condition = Condition::all();
    if filters.latest_turn_only {
        let generation = match filters.conversation_id {
            Some(id) => conversation::Entity::find_by_id(id)
                .one(conn)
                .await?
                .map(|row| row.last_completed_turn_generation)
                .filter(|generation| *generation > 0),
            None => None,
        };
        return Ok(condition
            .add(task_artifact::Column::ConversationId.eq(filters.conversation_id.unwrap_or(0)))
            .add(task_artifact::Column::TurnGeneration.eq(generation.unwrap_or(0)))
            .add(Expr::value(generation.is_some())));
    }
    if let Some(id) = filters.conversation_id {
        condition = if filters.include_related_conversations {
            let ids = conversation_scope_ids(conn, id).await?;
            tracing::debug!(
                conversation_id = id,
                scope_ids = ids.len(),
                "[task-artifacts] current conversation scope resolved"
            );
            condition.add(task_artifact::Column::ConversationId.is_in(ids))
        } else {
            condition.add(task_artifact::Column::ConversationId.eq(id))
        };
    }
    Ok(condition)
}

fn search_condition(search: &str) -> Condition {
    let escaped = search
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    Condition::any()
        .add(
            Func::lower(Expr::col((
                task_artifact::Entity,
                task_artifact::Column::DisplayName,
            )))
            .like(LikeExpr::new(pattern.clone()).escape('\\')),
        )
        .add(
            Func::lower(Expr::col((
                task_artifact::Entity,
                task_artifact::Column::Path,
            )))
            .like(LikeExpr::new(pattern.clone()).escape('\\')),
        )
        .add(
            Func::lower(Expr::col((
                conversation::Entity,
                conversation::Column::Title,
            )))
            .like(LikeExpr::new(pattern).escape('\\')),
        )
}

async fn fetch_page(
    conn: &DatabaseConnection,
    query: Select<task_artifact::Entity>,
    pagination: (u64, u64),
) -> Result<TaskArtifactPage, DbError> {
    let page_size = pagination.1.clamp(1, MAX_PAGE_SIZE);
    let total = query.clone().count(conn).await?;
    let total_pages = total.saturating_add(page_size - 1) / page_size;
    let page = pagination.0.max(1).min(total_pages.max(1));
    let rows = query
        .select_also(conversation::Entity)
        .paginate(conn, page_size)
        .fetch_page(page - 1)
        .await?;
    let mut items = Vec::with_capacity(rows.len());
    for (artifact, conversation) in rows {
        if let Some(conversation) = conversation {
            items.push(artifact_info(conn, artifact, conversation).await);
        }
    }
    Ok(TaskArtifactPage {
        items,
        total,
        page,
        page_size,
    })
}

pub(super) async fn artifact_info<C: ConnectionTrait>(
    conn: &C,
    artifact: task_artifact::Model,
    conversation: conversation::Model,
) -> TaskArtifactInfo {
    let current = current_artifact_state(&artifact.path, &artifact.kind);
    let last_checked_at = persist_current_state(conn, &artifact, &current).await;
    TaskArtifactInfo {
        id: artifact.id,
        conversation_id: artifact.conversation_id,
        message_id: artifact.message_id,
        folder_id: conversation.folder_id,
        conversation_title: conversation.title,
        agent_type: conversation.agent_type,
        path: artifact.path,
        display_name: artifact.display_name,
        kind: current.kind,
        created_at: artifact.created_at.to_rfc3339(),
        last_checked_at: last_checked_at.to_rfc3339(),
        status: current.status,
    }
}

async fn persist_current_state<C: ConnectionTrait>(
    conn: &C,
    artifact: &task_artifact::Model,
    current: &CurrentArtifactState,
) -> DateTime<Utc> {
    if current.status == artifact.status && current.kind == artifact.kind {
        return artifact.last_checked_at;
    }
    let now = Utc::now();
    // 状态探测结果仅回写同一版本，避免覆盖并发替换后的类型或状态。
    let result = task_artifact::Entity::update_many()
        .set(task_artifact::ActiveModel {
            status: Set(current.status.clone()),
            kind: Set(current.kind.clone()),
            last_checked_at: Set(now),
            ..Default::default()
        })
        .filter(task_artifact::Column::Id.eq(artifact.id))
        .filter(task_artifact::Column::Path.eq(&artifact.path))
        .filter(task_artifact::Column::LastCheckedAt.eq(artifact.last_checked_at))
        .exec(conn)
        .await;
    match result {
        Ok(result) if result.rows_affected > 0 => now,
        Ok(_) => artifact.last_checked_at,
        Err(error) => {
            tracing::warn!(artifact_id = artifact.id, conversation_id = artifact.conversation_id,
                error = %error, "[task-artifacts] state cache update failed");
            artifact.last_checked_at
        }
    }
}
