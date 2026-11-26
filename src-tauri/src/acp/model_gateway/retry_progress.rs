use std::sync::Arc;

use axum::http::HeaderMap;
use tokio::sync::RwLock;

use crate::{
    acp::{
        manager::ConnectionManager,
        runtime_observation::{RetryProgress, RuntimeObservation},
        types::AcpEvent,
        SessionState,
    },
    models::AgentType,
    web::event_bridge::{emit_with_state_gated, EventEmitter},
};

pub(super) struct RetryTurn {
    state: Arc<RwLock<SessionState>>,
    emitter: EventEmitter,
    external_id: String,
    generation: i64,
}

impl RetryTurn {
    pub(super) async fn bind(manager: &ConnectionManager, headers: &HeaderMap) -> Option<Self> {
        // Codex 的 ACP sessionId 对应模型请求的 thread-id；session-id 兼容旧版本。
        let (agent, external_id) = if let Some(id) = headers.get("x-claude-code-session-id") {
            (AgentType::ClaudeCode, id)
        } else {
            let id = headers
                .get("thread-id")
                .or_else(|| headers.get("session-id"))
                .or_else(|| headers.get("session_id"))?;
            (AgentType::Codex, id)
        };
        let external_id = external_id.to_str().ok()?;
        let id = manager
            .find_connection_by_external_id(external_id, agent)
            .await?;
        let (state, emitter) = manager.get_state_and_emitter(&id).await?;
        let generation = {
            let current = state.read().await;
            if !current.turn_in_flight || current.external_id.as_deref() != Some(external_id) {
                return None;
            }
            current.turn_generation
        };
        Some(Self {
            state,
            emitter,
            generation,
            external_id: external_id.to_owned(),
        })
    }

    fn matches(&self, state: &SessionState) -> bool {
        state.turn_in_flight
            && state.turn_generation == self.generation
            && state.external_id.as_deref() == Some(&self.external_id)
    }

    pub(super) async fn active(&self) -> bool {
        self.matches(&*self.state.read().await)
    }

    pub(super) async fn notify(&self, progress: RetryProgress) -> bool {
        let event = AcpEvent::RuntimeObservation {
            observation: RuntimeObservation::Retry(progress),
        };
        emit_with_state_gated(&self.state, &self.emitter, event, |state| {
            self.matches(state)
        })
        .await
    }
}
