use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, SqlErr,
    TransactionTrait,
};

use super::query::artifact_info;
use super::source::ResolvedArtifact;
use super::TaskArtifactInfo;
use crate::db::entities::{conversation, task_artifact};
use crate::db::error::DbError;

#[derive(Clone, Copy)]
pub(crate) struct ArtifactIdentity {
    pub conversation_id: i32,
    pub artifact_id: i32,
}

pub(crate) struct ArtifactUpdate {
    pub display_name: Option<String>,
    pub source: Option<ResolvedArtifact>,
}

pub(crate) async fn workspace_id<C: ConnectionTrait>(
    conn: &C,
    conversation_id: i32,
) -> Result<i32, DbError> {
    conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
        .map(|row| row.folder_id)
        .ok_or_else(|| DbError::NotFound("current conversation".into()))
}

pub(crate) async fn get_artifact<C: ConnectionTrait>(
    conn: &C,
    identity: ArtifactIdentity,
    all: bool,
) -> Result<Option<TaskArtifactInfo>, DbError> {
    let query =
        task_artifact::Entity::find_by_id(identity.artifact_id).inner_join(conversation::Entity);
    let query = if all {
        query.filter(
            conversation::Column::FolderId.eq(workspace_id(conn, identity.conversation_id).await?),
        )
    } else {
        query.filter(task_artifact::Column::ConversationId.eq(identity.conversation_id))
    };
    match query.select_also(conversation::Entity).one(conn).await? {
        Some((artifact, Some(conversation))) => {
            Ok(Some(artifact_info(conn, artifact, conversation).await))
        }
        _ => Ok(None),
    }
}

pub(crate) async fn update_artifact(
    conn: &DatabaseConnection,
    identity: ArtifactIdentity,
    changes: ArtifactUpdate,
) -> Result<TaskArtifactInfo, DbError> {
    let txn = conn.begin().await?;
    let existing = task_artifact::Entity::find_by_id(identity.artifact_id)
        .filter(task_artifact::Column::ConversationId.eq(identity.conversation_id))
        .one(&txn)
        .await?
        .ok_or_else(|| DbError::NotFound("artifact".into()))?;
    if let Some(source) = &changes.source {
        ensure_no_conflict(&txn, &existing, &source.path).await?;
    }
    task_artifact::Entity::update_many()
        .set(update_fields(changes))
        .filter(task_artifact::Column::Id.eq(identity.artifact_id))
        .filter(task_artifact::Column::ConversationId.eq(identity.conversation_id))
        .exec(&txn)
        .await
        .map_err(write_error)?;
    let result = get_artifact(&txn, identity, false)
        .await?
        .ok_or_else(|| DbError::NotFound("artifact".into()))?;
    if let Err(error) = txn.commit().await {
        tracing::error!(artifact_id = identity.artifact_id, conversation_id = identity.conversation_id,
            error = %error, "[task-artifacts] update commit outcome unknown");
        return Err(DbError::Validation("effect_unknown".into()));
    }
    Ok(result)
}

fn update_fields(changes: ArtifactUpdate) -> task_artifact::ActiveModel {
    let mut model = task_artifact::ActiveModel::default();
    if let Some(name) = changes.display_name {
        model.display_name = Set(name);
    }
    if let Some(source) = changes.source {
        model.path = Set(source.path);
        model.source_path = Set(source.source);
        model.kind = Set(source.kind);
        model.status = Set("available".into());
        model.last_checked_at = Set(Utc::now());
    }
    model
}

async fn ensure_no_conflict<C: ConnectionTrait>(
    conn: &C,
    existing: &task_artifact::Model,
    path: &str,
) -> Result<(), DbError> {
    let query = task_artifact::Entity::find()
        .filter(task_artifact::Column::ConversationId.eq(existing.conversation_id))
        .filter(task_artifact::Column::Path.eq(path))
        .filter(task_artifact::Column::Id.ne(existing.id));
    let query = match &existing.message_id {
        Some(id) => query.filter(task_artifact::Column::MessageId.eq(id)),
        None => query.filter(task_artifact::Column::MessageId.is_null()),
    };
    if query.one(conn).await?.is_some() {
        return Err(DbError::Validation("conflict".into()));
    }
    Ok(())
}

fn write_error(error: sea_orm::DbErr) -> DbError {
    match error.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => DbError::Validation("conflict".into()),
        _ => error.into(),
    }
}

pub(crate) async fn delete_artifact(
    conn: &DatabaseConnection,
    identity: ArtifactIdentity,
) -> Result<bool, DbError> {
    let result = task_artifact::Entity::delete_many()
        .filter(task_artifact::Column::Id.eq(identity.artifact_id))
        .filter(task_artifact::Column::ConversationId.eq(identity.conversation_id))
        .exec(conn)
        .await?;
    Ok(result.rows_affected > 0)
}
