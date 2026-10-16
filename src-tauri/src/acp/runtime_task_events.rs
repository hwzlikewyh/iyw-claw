use crate::acp::{session_state::SessionState, AcpEvent};
use crate::models::agent::AgentType;
use crate::web::event_bridge::{emit_with_state, EventEmitter};
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

pub(super) async fn await_goal_confirmation(
    context: (&Arc<RwLock<SessionState>>, &EventEmitter),
    binding: (&str, &super::RuntimeTask),
) {
    let revision = {
        let mut state = context.0.write().await;
        let Some(mut task) = state.runtime_tasks.get(&binding.1.id).cloned() else {
            return;
        };
        if state.external_id.as_deref() != Some(binding.0)
            || task.native_id != binding.1.native_id
            || task.updated_at != binding.1.updated_at
        {
            return;
        }
        task.status = "unknown".into();
        task.actions.clear();
        state.runtime_tasks.upsert(task);
        state.runtime_tasks.revision
    };
    emit_with_state(
        context.0,
        context.1,
        AcpEvent::RuntimeTasksChanged {
            revision,
            persistent: true,
        },
    )
    .await;
}

pub(crate) async fn observe_meta(
    context: (&Arc<RwLock<SessionState>>, &EventEmitter),
    agent: AgentType,
    meta: &Map<String, Value>,
) {
    let revision = {
        let mut state = context.0.write().await;
        let persistence = state.runtime_tasks.persistence_revision;
        state.runtime_tasks.observe_meta(agent, meta).then_some((
            state.runtime_tasks.revision,
            state.runtime_tasks.persistence_revision != persistence,
        ))
    };
    if let Some((revision, persistent)) = revision {
        emit_with_state(
            context.0,
            context.1,
            AcpEvent::RuntimeTasksChanged {
                revision,
                persistent,
            },
        )
        .await;
    }
}

pub(crate) async fn observe_sdk(
    context: (&Arc<RwLock<SessionState>>, &EventEmitter),
    message: &Value,
) {
    let revision = {
        let mut state = context.0.write().await;
        let persistence = state.runtime_tasks.persistence_revision;
        state.runtime_tasks.observe_sdk(message).then_some((
            state.runtime_tasks.revision,
            state.runtime_tasks.persistence_revision != persistence,
        ))
    };
    if let Some((revision, persistent)) = revision {
        emit_with_state(
            context.0,
            context.1,
            AcpEvent::RuntimeTasksChanged {
                revision,
                persistent,
            },
        )
        .await;
    }
}
