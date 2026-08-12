use std::sync::Arc;

use crate::acp::agent_input_dispatch::emit_current;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::{NativeBackgroundTurn, SessionState};
use crate::acp::types::ConnectionStatus;
use crate::acp::{AcpEvent, AgentInputStatus};
use crate::db::entities::conversation::ConversationStatus;
use crate::db::service::{agent_input_outbox_service, conversation_service};
use crate::db::AppDatabase;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

pub(crate) async fn finish_settlement(manager: &ConnectionManager, conn_id: &str, generation: i64) {
    let Some((state, emitter)) = manager.get_state_and_emitter(conn_id).await else {
        return;
    };
    let Some(background) = pending_turn(&state, generation).await else {
        finish_without_background(&state, generation).await;
        return;
    };
    let Some(db) = manager.agent_input_runtime.db() else {
        tracing::error!(
            connection_id = conn_id,
            generation,
            "[agent-input] cannot adopt native background turn without DB runtime"
        );
        return;
    };
    persist_consumption(&db, &state, &emitter, conn_id, generation, &background).await;
    let Some(adopted_generation) = adopt_generation(&state, conn_id, generation).await else {
        return;
    };
    mark_conversation_in_progress(&db, &state, &emitter, conn_id, adopted_generation).await;
    emit_adopted_turn(&state, &emitter, background).await;
    tracing::info!(
        connection_id = conn_id,
        source_generation = generation,
        adopted_generation,
        "[agent-input] adopted wrapper-owned native turn"
    );
}

async fn pending_turn(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    generation: i64,
) -> Option<NativeBackgroundTurn> {
    state
        .read()
        .await
        .native_background_turn
        .as_ref()
        .filter(|turn| turn.source_generation == generation && turn.adopted_generation.is_none())
        .cloned()
}

async fn finish_without_background(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    generation: i64,
) {
    let mut snapshot = state.write().await;
    if snapshot.turn_generation == generation {
        snapshot.turn_completion_pending = false;
        snapshot.agent_input_notify.notify_one();
    }
}

async fn persist_consumption(
    db: &Arc<AppDatabase>,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    conn_id: &str,
    generation: i64,
    background: &NativeBackgroundTurn,
) {
    match agent_input_outbox_service::consume_native_started_turn(
        &db.conn,
        &background.message_id,
        conn_id,
        generation,
    )
    .await
    {
        Ok(true) => emit_current(db, state, emitter, &background.message_id).await,
        Ok(false) => {
            tracing::error!(connection_id = conn_id, input_id = %background.message_id, generation, "[agent-input] native background turn lost its dispatch claim");
            fail_uncertain_consumption(db, state, emitter, background).await;
        }
        Err(error) => {
            tracing::error!(connection_id = conn_id, input_id = %background.message_id, generation, error = %error, "[agent-input] native background turn settlement failed");
            fail_uncertain_consumption(db, state, emitter, background).await;
        }
    }
}

async fn fail_uncertain_consumption(
    db: &Arc<AppDatabase>,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    background: &NativeBackgroundTurn,
) {
    match agent_input_outbox_service::transition_status(
        &db.conn,
        &background.message_id,
        AgentInputStatus::Dispatching,
        AgentInputStatus::Failed,
        Some("native_background_turn_consumption_persistence_failed".into()),
    )
    .await
    {
        Ok(true) => emit_current(db, state, emitter, &background.message_id).await,
        Ok(false) => {}
        Err(error) => {
            tracing::error!(input_id = %background.message_id, error = %error, "[agent-input] native background failure state could not be persisted")
        }
    }
}

async fn adopt_generation(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    conn_id: &str,
    generation: i64,
) -> Option<i64> {
    let mut snapshot = state.write().await;
    if snapshot.turn_generation != generation || !snapshot.turn_completion_pending {
        tracing::error!(
            connection_id = conn_id,
            generation,
            current_generation = snapshot.turn_generation,
            completion_pending = snapshot.turn_completion_pending,
            "[agent-input] native background adoption lost generation ownership"
        );
        return None;
    }
    let next_generation = generation.saturating_add(1);
    snapshot.turn_generation = next_generation;
    snapshot.turn_in_flight = true;
    snapshot.turn_completion_pending = false;
    snapshot.memory_turn_tracker.begin_accepted_turn();
    if let Some(turn) = snapshot.native_background_turn.as_mut() {
        turn.adopted_generation = Some(next_generation);
    }
    Some(next_generation)
}

async fn mark_conversation_in_progress(
    db: &AppDatabase,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    conn_id: &str,
    adopted_generation: i64,
) {
    let Some(conversation_id) = state.read().await.conversation_id else {
        return;
    };
    if let Err(error) = conversation_service::update_status(
        &db.conn,
        conversation_id,
        ConversationStatus::InProgress,
    )
    .await
    {
        tracing::error!(connection_id = conn_id, conversation_id, adopted_generation, error = %error, "[agent-input] native background conversation status update failed");
        return;
    }
    emit_with_state(
        state,
        emitter,
        AcpEvent::ConversationStatusChanged {
            conversation_id,
            status: ConversationStatus::InProgress,
        },
    )
    .await;
}

async fn emit_adopted_turn(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    background: NativeBackgroundTurn,
) {
    emit_with_state(
        state,
        emitter,
        AcpEvent::StatusChanged {
            status: ConnectionStatus::Prompting,
        },
    )
    .await;
    emit_with_state(
        state,
        emitter,
        AcpEvent::UserMessage {
            message_id: background.message_id,
            blocks: background.blocks,
        },
    )
    .await;
    let snapshot = state.read().await;
    snapshot.agent_input_notify.notify_one();
    snapshot.native_background_notify.notify_waiters();
}
