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
    if !background.automatic {
        persist_consumption(&db, &state, &emitter, conn_id, generation, &background).await;
    }
    let shared_home_connections = manager.hermes_shared_home_connection_count(conn_id).await;
    let Some(adopted_generation) =
        adopt_generation(&state, conn_id, generation, shared_home_connections).await
    else {
        return;
    };
    mark_conversation_in_progress(&db.conn, &state, &emitter, conn_id, adopted_generation).await;
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
    let mut snapshot = state.write().await;
    let pending = snapshot.native_background_turn
        .as_ref()
        .filter(|turn| turn.source_generation == generation && turn.adopted_generation.is_none())
        .cloned();
    if pending.is_none() && snapshot.turn_generation == generation {
        snapshot.turn_completion_pending = false;
        snapshot.agent_input_notify.notify_one();
    }
    pending
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
    hermes_shared_home_connections: Option<u16>,
) -> Option<i64> {
    let mut snapshot = state.write().await;
    let pending = snapshot.native_background_turn.as_ref().is_some_and(|turn| {
        turn.source_generation == generation && turn.adopted_generation.is_none()
    });
    if snapshot.turn_generation != generation || !snapshot.turn_completion_pending || !pending {
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
    let turn_nonce = snapshot.memory_turn_tracker.begin_accepted_turn();
    snapshot.begin_context_plan_receipt(turn_nonce, hermes_shared_home_connections);
    if let Some(turn) = snapshot.native_background_turn.as_mut() {
        turn.adopted_generation = Some(next_generation);
    }
    Some(next_generation)
}

async fn mark_conversation_in_progress(
    db: &sea_orm::DatabaseConnection,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    conn_id: &str,
    adopted_generation: i64,
) {
    let Some(conversation_id) = state.read().await.conversation_id else {
        return;
    };
    if let Err(error) = conversation_service::update_status(
        db,
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
    if !background.automatic {
        emit_with_state(state, emitter, AcpEvent::UserMessage {
            message_id: background.message_id, blocks: background.blocks,
        }).await;
    }
    let snapshot = state.read().await;
    snapshot.agent_input_notify.notify_one();
    snapshot.native_background_notify.notify_waiters();
}

pub(crate) async fn begin_automatic(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    request: (&sea_orm::DatabaseConnection, &str, Option<i64>),
) -> Result<Option<bool>, String> {
    let (db, turn_id, after_generation) = request;
    let (generation, idle, conn_id) = {
        let mut snapshot = state.write().await;
        if let Some(existing) = &snapshot.native_background_turn {
            return if existing.automatic && existing.message_id == turn_id { Ok(Some(true)) }
                else if existing.adopted_generation == Some(snapshot.turn_generation) { Ok(None) }
                else { Err("another native turn is already registered".into()) };
        }
        if after_generation.is_some_and(|generation| snapshot.turn_generation > generation) {
            return Ok(Some(false));
        }
        if after_generation.is_some_and(|generation| snapshot.turn_generation < generation) {
            return Err("automatic turn refers to a future host generation".into());
        }
        let idle = !snapshot.turn_in_flight && !snapshot.turn_completion_pending;
        let generation = snapshot.turn_generation;
        snapshot.native_background_turn = Some(NativeBackgroundTurn {
            automatic: true, message_id: turn_id.to_string(), blocks: Vec::new(),
            source_generation: generation, adopted_generation: None, terminal_status: None,
        });
        if idle { snapshot.turn_completion_pending = true; }
        (generation, idle, snapshot.connection_id.clone())
    };
    if idle {
        let adopted = adopt_generation(state, &conn_id, generation, None).await
            .ok_or("automatic turn lost its generation")?;
        mark_conversation_in_progress(db, state, emitter, &conn_id, adopted).await;
        let background = state.read().await.native_background_turn.clone().ok_or("automatic turn disappeared")?;
        emit_adopted_turn(state, emitter, background).await;
    }
    Ok(Some(true))
}

/// 只撤销本次未接管的登记；返回 true 表示它已在超时边界完成接管。
pub(crate) async fn cancel_pending_automatic(
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    turn_id: &str,
) -> bool {
    let mut snapshot = state.write().await;
    let Some(turn) = snapshot.native_background_turn.as_ref()
        .filter(|turn| turn.automatic && turn.message_id == turn_id) else {
        return false;
    };
    if turn.adopted_generation.is_some() {
        return true;
    }
    snapshot.native_background_turn = None;
    snapshot.native_background_notify.notify_waiters();
    snapshot.agent_input_notify.notify_one();
    false
}
