use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::SessionState;
use crate::acp::{AcpEvent, AgentInputItem, AgentInputPayload, AgentInputStrategy};
use crate::db::service::agent_input_outbox_service;
use crate::db::AppDatabase;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

#[derive(Default)]
pub(crate) struct AgentInputRuntime {
    db: OnceLock<Arc<AppDatabase>>,
    workers: Mutex<HashSet<String>>,
}

impl AgentInputRuntime {
    pub fn install_db(&self, db: Arc<AppDatabase>) {
        let _ = self.db.set(db);
    }

    pub fn db(&self) -> Option<Arc<AppDatabase>> {
        self.db.get().cloned()
    }

    pub async fn submit(
        self: &Arc<Self>,
        manager: &ConnectionManager,
        conn_id: &str,
        conversation_id: i32,
        id: String,
        payload: AgentInputPayload,
    ) -> Result<AgentInputItem, AcpError> {
        let db = self
            .db()
            .ok_or_else(|| AcpError::protocol("agent input database unavailable"))?;
        let (state, emitter) = manager
            .get_state_and_emitter(conn_id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
        let agent_type = validate_target(&state, conversation_id).await?;
        let item =
            agent_input_outbox_service::create(&db.conn, id, conversation_id, agent_type, payload)
                .await
                .map_err(|error| AcpError::protocol(error.to_string()))?;
        emit_input(&state, &emitter, item.clone()).await;
        self.ensure_worker(manager.clone_ref(), conn_id.to_string())
            .await;
        Ok(item)
    }

    pub(crate) async fn ensure_worker(
        self: &Arc<Self>,
        manager: ConnectionManager,
        conn_id: String,
    ) {
        if !self.workers.lock().await.insert(conn_id.clone()) {
            return;
        }
        let runtime = Arc::clone(self);
        tokio::spawn(async move {
            crate::acp::agent_input_worker::run(&runtime, &manager, &conn_id).await;
            runtime.workers.lock().await.remove(&conn_id);
        });
    }
}

async fn validate_target(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    conversation_id: i32,
) -> Result<crate::models::AgentType, AcpError> {
    let snapshot = state.read().await;
    if snapshot.conversation_id != Some(conversation_id) {
        return Err(AcpError::protocol(
            "agent input conversation does not match connection",
        ));
    }
    Ok(snapshot.agent_type)
}

pub(crate) async fn emit_current(
    db: &Arc<AppDatabase>,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    id: &str,
) {
    match agent_input_outbox_service::get(&db.conn, id).await {
        Ok(Some(item)) => emit_input(state, emitter, item).await,
        Ok(None) => {}
        Err(error) => {
            tracing::error!(input_id = id, error = %error, "[agent-input] failed to reload state");
        }
    }
}

pub(crate) async fn emit_input(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    item: AgentInputItem,
) {
    tracing::info!(
        input_id = %item.id,
        conversation_id = item.conversation_id,
        status = item.status.as_str(),
        strategy = item.strategy.map(AgentInputStrategy::as_str),
        payload_blocks = item.payload.blocks.len(),
        "[agent-input] state changed"
    );
    emit_with_state(state, emitter, AcpEvent::AgentInputChanged { item }).await;
}

impl ConnectionManager {
    pub async fn submit_agent_input(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        conversation_id: i32,
        id: String,
        payload: AgentInputPayload,
    ) -> Result<AgentInputItem, AcpError> {
        self.agent_input_runtime.install_db(Arc::new(AppDatabase {
            conn: db.conn.clone(),
        }));
        self.agent_input_runtime
            .submit(self, conn_id, conversation_id, id, payload)
            .await
    }

    pub async fn request_safe_cancel(
        &self,
        conn_id: &str,
        expected_turn_generation: i64,
    ) -> Result<(), AcpError> {
        let cmd_tx = {
            let connections = self.connections.lock().await;
            connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?
                .cmd_tx
                .clone()
        };
        cmd_tx
            .send(crate::acp::connection::ConnectionCommand::SafeCancel {
                expected_turn_generation,
            })
            .await
            .map_err(|_| AcpError::ProcessExited)
    }

    pub async fn delete_agent_input(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        conversation_id: i32,
        id: &str,
    ) -> Result<AgentInputItem, AcpError> {
        let (state, emitter) = self
            .get_state_and_emitter(conn_id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
        validate_target(&state, conversation_id).await?;
        let existing = agent_input_outbox_service::get(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?
            .ok_or_else(|| AcpError::protocol("agent input not found"))?;
        if existing.conversation_id != conversation_id {
            return Err(AcpError::protocol(
                "agent input conversation does not match connection",
            ));
        }
        let changed = agent_input_outbox_service::delete_waiting(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        if !changed {
            return Err(AcpError::protocol("agent input can no longer be deleted"));
        }
        let item = agent_input_outbox_service::get(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?
            .ok_or_else(|| AcpError::protocol("agent input not found"))?;
        emit_input(&state, &emitter, item.clone()).await;
        Ok(item)
    }

    pub async fn retry_agent_input(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        conversation_id: i32,
        id: &str,
    ) -> Result<AgentInputItem, AcpError> {
        let (state, emitter) = self
            .get_state_and_emitter(conn_id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
        validate_target(&state, conversation_id).await?;
        let existing = agent_input_outbox_service::get(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?
            .ok_or_else(|| AcpError::protocol("agent input not found"))?;
        if existing.conversation_id != conversation_id {
            return Err(AcpError::protocol(
                "agent input conversation does not match connection",
            ));
        }
        let changed = agent_input_outbox_service::retry_failed(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        if !changed {
            return Err(AcpError::protocol("agent input is not retryable"));
        }
        let item = agent_input_outbox_service::get(&db.conn, id)
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?
            .ok_or_else(|| AcpError::protocol("agent input not found"))?;
        emit_input(&state, &emitter, item.clone()).await;
        self.agent_input_runtime
            .ensure_worker(self.clone_ref(), conn_id.to_owned())
            .await;
        Ok(item)
    }

    pub(crate) async fn resume_agent_inputs(&self, db: &DatabaseConnection, conn_id: &str) {
        self.agent_input_runtime
            .install_db(Arc::new(AppDatabase { conn: db.clone() }));
        self.agent_input_runtime
            .ensure_worker(self.clone_ref(), conn_id.to_owned())
            .await;
    }

    pub async fn finish_agent_input_turn_settlement(&self, conn_id: &str, generation: i64) {
        let Some(state) = self.get_state(conn_id).await else {
            return;
        };
        let mut snapshot = state.write().await;
        if snapshot.turn_generation == generation {
            snapshot.turn_completion_pending = false;
            snapshot.agent_input_notify.notify_one();
        }
    }
}
