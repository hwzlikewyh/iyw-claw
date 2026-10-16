use super::{RuntimeTaskSnapshot, RuntimeTasks};
use crate::acp::{error::AcpError, manager::ConnectionManager, runtime_task_receipts};
use crate::db::service::{conversation_service, conversation_session_segment_service};

impl ConnectionManager {
    pub async fn runtime_tasks_for(
        &self,
        connection: Option<&str>,
        conversation: Option<i32>,
    ) -> Result<RuntimeTaskSnapshot, AcpError> {
        if let Some(id) = connection {
            match self.runtime_task_snapshot(id).await {
                Ok(snapshot) => return Ok(snapshot),
                Err(AcpError::ConnectionNotFound(_)) if conversation.is_some() => {}
                Err(error) => return Err(error),
            }
        }
        let Some(conversation) = conversation else {
            return Ok(RuntimeTaskSnapshot::default());
        };
        let db = self
            .agent_input_runtime
            .db()
            .ok_or_else(|| AcpError::protocol("Task database is unavailable"))?;
        let summary = conversation_service::get_by_id(&db.conn, conversation)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        let mut ids: std::collections::BTreeSet<_> = summary.external_id.into_iter().collect();
        let segments =
            conversation_session_segment_service::list_for_conversation(&db.conn, conversation)
                .await
                .map_err(|error| AcpError::protocol(error.to_string()))?;
        ids.extend(
            segments
                .into_iter()
                .filter_map(|segment| segment.external_id),
        );
        let mut restored = RuntimeTasks::default();
        for id in ids {
            if let Some(snapshot) =
                runtime_task_receipts::load_tasks(&db.conn, summary.agent_type, &id).await
            {
                restored.restore(snapshot);
            }
        }
        Ok(restored.snapshot())
    }
}
