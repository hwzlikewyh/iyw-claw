use serde_json::{json, Value};

use super::artifact_tool::ArtifactContext;
use super::listener::{DelegationListener, TokenEntry};
use super::transport::BrokerArtifactManagementRequest;

impl DelegationListener {
    pub(super) async fn process_artifact_management(
        &self,
        mut request: BrokerArtifactManagementRequest,
    ) -> Value {
        let action = request.operation.action();
        if let Err(message) = request.operation.validate() {
            return json!({"action": action, "error": "invalid_input", "message": message});
        }
        let Some(entry) = self.tokens.lookup(&request.token).await else {
            return json!({"action": action, "error": "invalid_session"});
        };
        let Some(context) = self.artifact_context(&entry).await else {
            return json!({"action": action, "error": "session_not_ready"});
        };
        if !request.operation.mutates() {
            return self
                .artifacts
                .manage_task_artifacts(context, request.operation)
                .await;
        }
        let Some(mutation) = self
            .tokens
            .acquire_mutation_commit(&request.token, &entry)
            .await
        else {
            return json!({"action": action, "error": "invalid_session"});
        };
        let artifacts = self.artifacts.clone();
        let conversation_id = context.conversation_id;
        // 写入任务持有提交许可，调用者取消等待时仍完成文件与数据库的一致性处理。
        tokio::spawn(async move {
            let _mutation = mutation;
            artifacts.manage_task_artifacts(context, request.operation).await
        }).await.unwrap_or_else(|error| {
            tracing::error!(action, conversation_id, error = %error, "[task-artifacts] mutation task failed");
            json!({"action": action, "error": "effect_unknown", "effectMayHaveOccurred": true})
        })
    }

    async fn artifact_context(&self, entry: &TokenEntry) -> Option<ArtifactContext> {
        Some(ArtifactContext {
            conversation_id: self
                .parent_lookup
                .current_conversation_id(&entry.parent_connection_id)
                .await?,
            turn_generation: self
                .parent_lookup
                .current_turn_generation(&entry.parent_connection_id)
                .await,
            connection_id: entry.parent_connection_id.clone(),
            working_dir: entry.working_dir.clone(),
        })
    }
}
