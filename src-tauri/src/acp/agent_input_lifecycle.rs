use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::acp::agent_input_dispatch::emit_current;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::SessionState;
use crate::acp::{AgentInputStatus, AgentInputStrategy};
use crate::db::service::agent_input_outbox_service;
use crate::db::AppDatabase;
use crate::web::event_bridge::EventEmitter;

/// Rehydrate durable inputs when a connection becomes associated with a
/// conversation. This runs for both a live reconnect and a fresh process that
/// resumes a persisted Agent session. A previous `dispatching` claim cannot be
/// trusted across process boundaries; keep it only when the same turn is still
/// observable, otherwise return it to the FIFO queue.
pub(crate) async fn recover_connection(
    db: &DatabaseConnection,
    manager: &ConnectionManager,
    connection_id: &str,
    conversation_id: i32,
) -> Result<(), crate::db::error::DbError> {
    let Some((state, emitter)) = manager.get_state_and_emitter(connection_id).await else {
        return Ok(());
    };
    let (turn_in_flight, turn_generation) = {
        let snapshot = state.read().await;
        (snapshot.turn_in_flight, snapshot.turn_generation)
    };
    let items = agent_input_outbox_service::list_recoverable(db, conversation_id).await?;
    for item in &items {
        if item.status == AgentInputStatus::Dispatching
            && (!turn_in_flight || item.target_turn_generation != Some(turn_generation))
        {
            let changed = agent_input_outbox_service::transition_status(
                db,
                &item.id,
                AgentInputStatus::Dispatching,
                AgentInputStatus::FallbackQueued,
                Some("dispatch_claim_recovered_after_connection_restart".into()),
            )
            .await?;
            if changed {
                emit_current(&db_handle(db), &state, &emitter, &item.id).await;
            }
        } else {
            emit_current(&db_handle(db), &state, &emitter, &item.id).await;
        }
    }
    manager.resume_agent_inputs(db, connection_id).await;
    Ok(())
}

pub(crate) async fn consume_user_message(
    db: &DatabaseConnection,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    message_id: &str,
) -> Result<(), crate::db::error::DbError> {
    let Some(item) = agent_input_outbox_service::get(db, message_id).await? else {
        return Ok(());
    };
    if item.status != AgentInputStatus::Dispatching {
        return Ok(());
    }
    let changed = agent_input_outbox_service::transition_status(
        db,
        message_id,
        AgentInputStatus::Dispatching,
        AgentInputStatus::Consumed,
        None,
    )
    .await?;
    if changed {
        emit_current(&db_handle(db), state, emitter, message_id).await;
    }
    Ok(())
}

pub(crate) async fn fallback_unconsumed_turn(
    db: &DatabaseConnection,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    connection_id: &str,
    generation: i64,
) -> Result<(), crate::db::error::DbError> {
    let items =
        agent_input_outbox_service::list_dispatching_for_turn(db, connection_id, generation)
            .await?;
    for item in items {
        let (status, reason) = match item.strategy {
            Some(AgentInputStrategy::DeferredNext) => (AgentInputStatus::Consumed, None),
            Some(AgentInputStrategy::CooperativeFeedback)
            | Some(AgentInputStrategy::NativeSteer) => (
                AgentInputStatus::FallbackQueued,
                Some("turn_completed_before_input_consumption".into()),
            ),
            None => continue,
        };
        let changed = agent_input_outbox_service::transition_status(
            db,
            &item.id,
            AgentInputStatus::Dispatching,
            status,
            reason,
        )
        .await?;
        if changed {
            emit_current(&db_handle(db), state, emitter, &item.id).await;
        }
    }
    Ok(())
}

pub(crate) async fn filter_feedback_commit_ids(
    manager: &ConnectionManager,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    ids: Vec<String>,
) -> Vec<String> {
    let Some(db) = manager.agent_input_runtime.db() else {
        return ids;
    };
    let mut accepted = Vec::with_capacity(ids.len());
    for id in ids {
        match consume_feedback_id(&db, state, emitter, &id).await {
            Ok(true) => accepted.push(id),
            Ok(false) => {}
            Err(error) => {
                tracing::error!(input_id = id, error = %error, "[agent-input] feedback commit persistence failed");
            }
        }
    }
    accepted
}

async fn consume_feedback_id(
    db: &Arc<AppDatabase>,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    id: &str,
) -> Result<bool, crate::db::error::DbError> {
    let Some(item) = agent_input_outbox_service::get(&db.conn, id).await? else {
        return Ok(true);
    };
    if item.status == AgentInputStatus::Consumed {
        return Ok(true);
    }
    if !matches!(
        item.status,
        AgentInputStatus::Dispatching | AgentInputStatus::FallbackQueued
    ) {
        return Ok(false);
    }
    if item.status == AgentInputStatus::Dispatching
        && item.strategy != Some(AgentInputStrategy::CooperativeFeedback)
    {
        return Ok(false);
    }
    let changed = agent_input_outbox_service::transition_status(
        &db.conn,
        id,
        item.status,
        AgentInputStatus::Consumed,
        None,
    )
    .await?;
    if changed {
        emit_current(db, state, emitter, id).await;
    }
    Ok(changed)
}

fn db_handle(db: &DatabaseConnection) -> Arc<AppDatabase> {
    Arc::new(AppDatabase { conn: db.clone() })
}
