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
        if let Some(batch_id) = item.force_batch_id.as_deref() {
            let uncertain = item.status == AgentInputStatus::Dispatching;
            if uncertain {
                let batch = agent_input_outbox_service::list_force_batch(db, batch_id).await?;
                agent_input_outbox_service::fail_force_batch(
                    db,
                    batch_id,
                    "force batch result unknown after connection restart".into(),
                )
                .await?;
                for member in batch {
                    emit_current(&db_handle(db), &state, &emitter, &member.id).await;
                }
                continue;
            }
        }
        if item.status == AgentInputStatus::Dispatching
            && (!turn_in_flight || item.target_turn_generation != Some(turn_generation))
        {
            let uncertain_native = item.strategy == Some(AgentInputStrategy::NativeSteer);
            let changed = agent_input_outbox_service::transition_status(
                db,
                &item.id,
                AgentInputStatus::Dispatching,
                if uncertain_native {
                    AgentInputStatus::Failed
                } else {
                    AgentInputStatus::FallbackQueued
                },
                Some(
                    if uncertain_native {
                        "native_steer_result_unknown_after_connection_restart"
                    } else {
                        "dispatch_claim_recovered_after_connection_restart"
                    }
                    .into(),
                ),
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
    if item.force_batch_id.is_some() {
        // Force batches use the connection loop's explicit prompt-acceptance
        // acknowledgement. UserMessage is a presentation event and can be
        // emitted before the ACP request is accepted.
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
    manager: &ConnectionManager,
    state: &Arc<tokio::sync::RwLock<SessionState>>,
    emitter: &EventEmitter,
    connection_id: &str,
    generation: i64,
) -> Result<(), crate::db::error::DbError> {
    let dispatch_lock = manager
        .agent_input_runtime
        .dispatch_lock(connection_id)
        .await;
    let _dispatch_guard = dispatch_lock.lock().await;
    let items =
        agent_input_outbox_service::list_dispatching_for_turn(db, connection_id, generation)
            .await?;
    let native_background_id = {
        let snapshot = state.read().await;
        snapshot
            .native_background_turn
            .as_ref()
            .filter(|turn| turn.source_generation == generation)
            .map(|turn| turn.message_id.clone())
    };
    let mut settled_force_batches = std::collections::HashSet::new();
    for item in items {
        if native_background_id.as_deref() == Some(item.id.as_str()) {
            continue;
        }
        let (status, reason) = match item.strategy {
            Some(AgentInputStrategy::DeferredNext) => (AgentInputStatus::Consumed, None),
            Some(AgentInputStrategy::CooperativeFeedback) => (
                AgentInputStatus::FallbackQueued,
                Some("turn_completed_before_input_consumption".into()),
            ),
            Some(AgentInputStrategy::NativeSteer) => (
                AgentInputStatus::Failed,
                Some("native_steer_result_unknown_at_turn_completion".into()),
            ),
            Some(AgentInputStrategy::SafeForceNext) => {
                let Some(batch_id) = item.force_batch_id.as_deref() else {
                    tracing::error!(input_id = %item.id, generation, "[agent-input] force dispatch lost its batch identity");
                    continue;
                };
                if settled_force_batches.insert(batch_id.to_string()) {
                    let members =
                        agent_input_outbox_service::list_force_batch(db, batch_id).await?;
                    agent_input_outbox_service::transition_force_batch(
                        db,
                        crate::db::service::agent_input_force_service::ForceBatchTransition {
                            batch_id,
                            turn_generation: generation,
                            from: AgentInputStatus::Dispatching,
                            to: AgentInputStatus::Consumed,
                            error: None,
                        },
                    )
                    .await?;
                    for member in members {
                        emit_current(&db_handle(db), state, emitter, &member.id).await;
                    }
                }
                continue;
            }
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
    let connection_id = state.read().await.connection_id.clone();
    let dispatch_lock = manager
        .agent_input_runtime
        .dispatch_lock(&connection_id)
        .await;
    let _dispatch_guard = dispatch_lock.lock().await;
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
    if item.force_batch_id.is_some() {
        return Ok(false);
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
