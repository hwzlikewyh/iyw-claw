use std::collections::HashSet;
use std::sync::Arc;

use crate::acp::agent_input_dispatch::{emit_input, validate_target};
use crate::acp::error::AcpError;
use crate::acp::feedback::FeedbackStatus;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::SessionState;
use crate::acp::{AcpEvent, AgentInputItem, AgentInputStatus};
use crate::db::service::{agent_input_ordering_service, agent_input_outbox_service};
use crate::db::AppDatabase;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

impl ConnectionManager {
    pub async fn reorder_agent_inputs(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        conversation_id: i32,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<AgentInputItem>, AcpError> {
        let (state, emitter) = self.target(conn_id, conversation_id).await?;
        let items = agent_input_ordering_service::reorder(&db.conn, conversation_id, &ordered_ids)
            .await
            .map_err(db_error)?;
        emit_items(&state, &emitter, &items).await;
        Ok(items)
    }

    pub async fn force_agent_inputs_through(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        conversation_id: i32,
        target_id: &str,
        expected_prefix_ids: Vec<String>,
    ) -> Result<Vec<AgentInputItem>, AcpError> {
        let dispatch_lock = self.agent_input_runtime.dispatch_lock(conn_id).await;
        let _dispatch_guard = dispatch_lock.lock().await;
        let (state, emitter) = self.target(conn_id, conversation_id).await?;
        let Some(items) =
            freeze_force_prefix(db, conversation_id, target_id, &expected_prefix_ids).await?
        else {
            return agent_input_outbox_service::list_visible(&db.conn, conversation_id)
                .await
                .map_err(db_error);
        };
        withdraw_pending_feedback(&state, &emitter, &items).await;
        emit_items(&state, &emitter, &items).await;
        state.read().await.agent_input_notify.notify_one();
        self.agent_input_runtime
            .ensure_worker(self.clone_ref(), conn_id.to_owned())
            .await;
        Ok(items)
    }

    async fn target(
        &self,
        conn_id: &str,
        conversation_id: i32,
    ) -> Result<(Arc<tokio::sync::RwLock<SessionState>>, EventEmitter), AcpError> {
        let target = self
            .get_state_and_emitter(conn_id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
        validate_target(&target.0, conversation_id).await?;
        Ok(target)
    }
}

async fn freeze_force_prefix(
    db: &AppDatabase,
    conversation_id: i32,
    target_id: &str,
    expected_prefix_ids: &[String],
) -> Result<Option<Vec<AgentInputItem>>, AcpError> {
    let target = agent_input_outbox_service::get(&db.conn, target_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| AcpError::protocol("agent input target not found"))?;
    if target.conversation_id != conversation_id {
        return Err(AcpError::protocol(
            "agent input conversation does not match connection",
        ));
    }
    if target.status == AgentInputStatus::Consumed {
        return Ok(None);
    }
    let batch_id = uuid::Uuid::new_v4().to_string();
    agent_input_ordering_service::freeze_prefix(
        &db.conn,
        agent_input_ordering_service::FreezePrefixRequest {
            conversation_id,
            target_id,
            expected_prefix_ids,
            batch_id: &batch_id,
        },
    )
    .await
    .map(Some)
    .map_err(db_error)
}

async fn withdraw_pending_feedback(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    items: &[AgentInputItem],
) {
    let batch_ids = items.iter().map(|item| &item.id).collect::<HashSet<_>>();
    let withdrawn = state
        .read()
        .await
        .feedback
        .iter()
        .filter(|feedback| {
            feedback.status == FeedbackStatus::Pending && batch_ids.contains(&feedback.id)
        })
        .map(|feedback| feedback.id.clone())
        .collect::<Vec<_>>();
    if !withdrawn.is_empty() {
        emit_with_state(
            state,
            emitter,
            AcpEvent::FeedbackWithdrawn { ids: withdrawn },
        )
        .await;
    }
}

async fn emit_items(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    items: &[AgentInputItem],
) {
    for item in items {
        emit_input(state, emitter, item.clone()).await;
    }
}

fn db_error(error: impl ToString) -> AcpError {
    AcpError::protocol(error.to_string())
}
