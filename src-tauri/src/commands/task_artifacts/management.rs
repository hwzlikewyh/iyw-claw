use std::time::Instant;

use serde_json::{json, Value};

use super::{emit_task_artifact_change, replacement, DbTaskArtifactAccess};
use crate::acp::delegation::artifact_tool::{
    ArtifactContext, ArtifactListQuery, ArtifactOperation, ArtifactScope,
};
use crate::db::error::DbError;
use crate::db::service::task_artifact_service::{management as service, query, DEFAULT_PAGE_SIZE};
use service::{ArtifactIdentity, ArtifactUpdate};

impl DbTaskArtifactAccess {
    pub(super) async fn manage(
        &self,
        context: ArtifactContext,
        mut operation: ArtifactOperation,
    ) -> Value {
        let action = operation.action();
        let id = operation.artifact_id();
        let started = Instant::now();
        let result = match operation.validate() {
            Ok(()) => self.execute_management(&context, operation).await,
            Err(message) => Err(DbError::Validation(message)),
        };
        match result {
            Ok(mut result) => {
                result["action"] = json!(action);
                tracing::info!(
                    action,
                    conversation_id = context.conversation_id,
                    artifact_id = id,
                    elapsed_ms = started.elapsed().as_millis(),
                    "[task-artifacts] management completed"
                );
                result
            }
            Err(error) => {
                tracing::error!(action, conversation_id = context.conversation_id, artifact_id = id,
                    elapsed_ms = started.elapsed().as_millis(), error = %error,
                    "[task-artifacts] management failed");
                error_result(action, error)
            }
        }
    }

    async fn execute_management(
        &self,
        context: &ArtifactContext,
        operation: ArtifactOperation,
    ) -> Result<Value, DbError> {
        match operation {
            ArtifactOperation::List(filters) => self.query_artifacts(context, filters).await,
            ArtifactOperation::Get { artifact_id, scope } => {
                let identity = ArtifactIdentity {
                    conversation_id: context.conversation_id,
                    artifact_id,
                };
                let artifact = service::get_artifact(
                    &self.db.conn,
                    identity,
                    matches!(scope, ArtifactScope::All),
                )
                .await?
                .ok_or_else(|| DbError::NotFound("artifact".into()))?;
                Ok(json!(artifact))
            }
            ArtifactOperation::Update {
                artifact_id,
                display_name,
                source,
            } => {
                self.update_artifact(context, (artifact_id, display_name, source))
                    .await
            }
            ArtifactOperation::Delete { artifact_id } => {
                self.delete_artifact(context, artifact_id).await
            }
        }
    }

    async fn delete_artifact(
        &self,
        context: &ArtifactContext,
        artifact_id: i32,
    ) -> Result<Value, DbError> {
        let identity = ArtifactIdentity {
            conversation_id: context.conversation_id,
            artifact_id,
        };
        let deleted = service::delete_artifact(&self.db.conn, identity).await?;
        if deleted {
            emit_task_artifact_change(&self.emitter, context.conversation_id, "deleted");
        }
        Ok(json!({"artifact_id": artifact_id, "deleted": deleted}))
    }

    async fn query_artifacts(
        &self,
        context: &ArtifactContext,
        filters: ArtifactListQuery,
    ) -> Result<Value, DbError> {
        let (conversation_id, folder_id) = match filters.scope {
            ArtifactScope::Current => (Some(context.conversation_id), None),
            ArtifactScope::All => (
                None,
                Some(service::workspace_id(&self.db.conn, context.conversation_id).await?),
            ),
        };
        let query = query::ArtifactQuery {
            conversation_id,
            folder_id,
            message_id: filters.message_id.as_deref(),
            latest_turn_only: false,
            include_related_conversations: false,
            search: filters.search.as_deref(),
            page: filters.page.unwrap_or(1),
            page_size: filters.page_size.unwrap_or(DEFAULT_PAGE_SIZE),
        };
        Ok(json!(query::query_artifacts(&self.db.conn, query).await?))
    }

    async fn update_artifact(
        &self,
        context: &ArtifactContext,
        changes: (i32, Option<String>, Option<String>),
    ) -> Result<Value, DbError> {
        let (artifact_id, display_name, source) = changes;
        let identity = ArtifactIdentity {
            conversation_id: context.conversation_id,
            artifact_id,
        };
        service::get_artifact(&self.db.conn, identity, false)
            .await?
            .ok_or_else(|| DbError::NotFound("artifact".into()))?;
        let (source, staging) = match source {
            Some(source) => {
                let prepared = replacement::prepare(context, source).await?;
                (Some(prepared.source), prepared.directory)
            }
            None => (None, None),
        };
        let result = service::update_artifact(
            &self.db.conn,
            identity,
            ArtifactUpdate {
                display_name,
                source,
            },
        )
        .await;
        if result.is_ok()
            || matches!(&result, Err(DbError::Validation(code)) if code == "effect_unknown")
        {
            if let Some(directory) = staging {
                let _ = directory.keep();
            }
            emit_task_artifact_change(&self.emitter, context.conversation_id, "updated");
        }
        result.map(|artifact| json!(artifact))
    }
}

fn error_result(action: &str, error: DbError) -> Value {
    let code = match &error {
        DbError::NotFound(_) => "not_found",
        DbError::Validation(code) => code.as_str(),
        DbError::Io(_) => "materialize_failed",
        _ => "persistence_failed",
    };
    let mut result = json!({"action": action, "error": code});
    if code == "effect_unknown" {
        result["effectMayHaveOccurred"] = json!(true);
    }
    result
}
