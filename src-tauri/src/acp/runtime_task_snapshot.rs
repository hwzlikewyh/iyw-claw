use super::{RuntimeTask, RuntimeTaskSnapshot};
use crate::acp::{
    error::AcpError,
    manager::ConnectionManager,
    session_state::{ActiveDelegationState, SessionState},
};
use std::sync::Arc;
use tokio::sync::RwLock;

impl ConnectionManager {
    pub async fn runtime_task_snapshot(&self, id: &str) -> Result<RuntimeTaskSnapshot, AcpError> {
        let state = self
            .get_state(id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(id.into()))?;
        self.restore_task_history(&state).await;
        let (mut result, delegations, connected) = {
            let state = state.read().await;
            (
                state.runtime_tasks.snapshot(),
                state
                    .active_delegations
                    .values()
                    .cloned()
                    .collect::<Vec<_>>(),
                !matches!(state.status, crate::acp::ConnectionStatus::Disconnected | crate::acp::ConnectionStatus::Error),
            )
        };
        if !connected {
            for task in &mut result.tasks {
                if !super::is_terminal(&task.status) { task.status = "unknown".into(); }
                task.actions.clear();
            }
        }
        for child in delegations {
            let task_id = format!("delegation:{}", child.parent_tool_use_id);
            let previous = result
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .cloned()
                .unwrap_or_default();
            result.tasks.retain(|task| task.id != task_id);
            result
                .tasks
                .push(self.delegation_task(child, previous).await);
        }
        Ok(result)
    }

    async fn restore_task_history(&self, state: &Arc<RwLock<SessionState>>) {
        let restore = {
            let mut snapshot = state.write().await;
            if snapshot.runtime_tasks.restored || snapshot.external_id.is_none() {
                None
            } else {
                snapshot.runtime_tasks.restored = true;
                snapshot
                    .external_id
                    .clone()
                    .map(|external| (snapshot.agent_type, external))
            }
        };
        if let (Some((agent, external)), Some(db)) = (restore, self.agent_input_runtime.db()) {
            if let Some(snapshot) =
                crate::acp::runtime_task_receipts::load_tasks(&db.conn, agent, &external).await
            {
                let mut current = state.write().await;
                if current.agent_type == agent
                    && current.external_id.as_deref() == Some(external.as_str())
                {
                    current.runtime_tasks.restore(snapshot);
                }
            }
        }
    }

    async fn delegation_task(
        &self,
        child: ActiveDelegationState,
        previous: RuntimeTask,
    ) -> RuntimeTask {
        let status = if let Some(state) = self.get_state(&child.child_connection_id).await {
            let state = state.read().await;
            if state.pending_permission.is_some() || state.pending_question.is_some() {
                "blocked"
            } else if state.turn_in_flight {
                "running"
            } else {
                "starting"
            }
        } else {
            "unknown"
        };
        RuntimeTask {
            id: format!("delegation:{}", child.parent_tool_use_id),
            kind: "delegation".into(),
            native_id: child.child_connection_id.clone(),
            title: crate::acp::registry::get_agent_meta(child.agent_type)
                .name
                .into(),
            status: status.into(),
            actions: if matches!(status, "running" | "blocked") {
                vec!["stop".into()]
            } else {
                vec![]
            },
            tool_call_id: Some(child.parent_tool_use_id),
            child_conversation_id: Some(child.child_conversation_id),
            child_connection_id: Some(child.child_connection_id),
            agent_type: Some(child.agent_type),
            ..previous
        }
    }
}
