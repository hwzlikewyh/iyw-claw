use super::{text, RuntimeTask, RuntimeTasks};
use crate::acp::{session_state::ToolCallState, types::DelegationResultSummary, AcpEvent};
use std::collections::BTreeMap;

impl RuntimeTasks {
    pub(crate) fn observe_delegation(
        &mut self,
        event: &AcpEvent,
        tools: &BTreeMap<String, ToolCallState>,
    ) {
        let (tool, connection, conversation, agent) = match event {
            AcpEvent::DelegationStarted {
                parent_tool_use_id,
                child_connection_id,
                child_conversation_id,
                agent_type,
                ..
            }
            | AcpEvent::DelegationCompleted {
                parent_tool_use_id,
                child_connection_id,
                child_conversation_id,
                agent_type,
                ..
            } => (
                parent_tool_use_id,
                child_connection_id,
                child_conversation_id,
                agent_type,
            ),
            _ => return,
        };
        let id = format!("delegation:{tool}");
        let mut task = self.tasks.get(&id).cloned().unwrap_or_else(|| RuntimeTask {
            id,
            native_id: connection.clone(),
            kind: "delegation".into(),
            title: crate::acp::registry::get_agent_meta(*agent).name.into(),
            tool_call_id: Some(tool.clone()),
            child_connection_id: Some(connection.clone()),
            child_conversation_id: Some(*conversation),
            agent_type: Some(*agent),
            ..Default::default()
        });
        if let Some(input) = tools.get(tool).and_then(|tool| tool.input.as_ref()) {
            task.objective = text(input, &["task", "prompt"]).or(task.objective);
        }
        settle_delegation(&mut task, event);
        task.actions.clear();
        self.upsert(task);
    }
}

fn settle_delegation(task: &mut RuntimeTask, event: &AcpEvent) {
    match event {
        AcpEvent::DelegationStarted { .. } => {
            task.status = "starting".into();
        }
        AcpEvent::DelegationCompleted { result, .. } => match result {
            DelegationResultSummary::Ok { text_preview, .. } => {
                task.status = "completed".into();
                task.summary = text_preview.clone();
            }
            DelegationResultSummary::Err { error_code } => {
                task.status = if error_code == "canceled" {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                task.summary = Some(error_code.clone());
            }
        },
        _ => return,
    }
}
