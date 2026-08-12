use crate::acp::agent_input_dispatch::emit_current;
use crate::acp::agent_input_worker::{WorkerContext, WorkerSnapshot};
use crate::acp::{AgentInputItem, AgentInputStatus, AgentInputStrategy};
use crate::db::service::agent_input_outbox_service;

pub(super) async fn dispatch_force_batch(
    context: &WorkerContext<'_>,
    items: Vec<AgentInputItem>,
    snapshot: WorkerSnapshot,
) {
    let Some(batch_id) = items.first().and_then(|item| item.force_batch_id.clone()) else {
        return;
    };
    let dispatch_lock = context
        .manager
        .agent_input_runtime
        .dispatch_lock(context.conn_id)
        .await;
    let _dispatch_guard = dispatch_lock.lock().await;
    let target_generation = snapshot.turn_generation.saturating_add(1);
    match claim_batch(context, &batch_id, target_generation, items.len()).await {
        Ok(()) => {}
        Err(error) => {
            fail_force_batch(context, &batch_id, error).await;
            return;
        }
    }
    for item in &items {
        emit_current(context.db, context.state, context.emitter, &item.id).await;
    }
    if let Some(mode_id) = items.last().and_then(|item| item.payload.mode_id.clone()) {
        if let Err(error) = context.manager.set_mode(context.conn_id, mode_id).await {
            fail_force_batch(context, &batch_id, error.to_string()).await;
            return;
        }
    }
    let result = context
        .manager
        .send_force_batch_prompt(
            context.db,
            context.conn_id,
            &items,
            snapshot.folder_id,
            snapshot.conversation_id,
        )
        .await;
    match result {
        Ok(()) => consume_force_batch(context, &batch_id, target_generation, &items).await,
        Err(error) => fail_force_batch(context, &batch_id, error.to_string()).await,
    }
}

async fn claim_batch(
    context: &WorkerContext<'_>,
    batch_id: &str,
    target_generation: i64,
    expected: usize,
) -> Result<(), String> {
    match agent_input_outbox_service::mark_force_batch_dispatching(
        &context.db.conn,
        crate::db::service::agent_input_force_service::ForceBatchClaim {
            batch_id,
            connection_id: context.conn_id,
            turn_generation: target_generation,
        },
    )
    .await
    {
        Ok(count) if count == expected as u64 => Ok(()),
        Ok(count) => {
            tracing::warn!(
                batch_id,
                expected,
                claimed = count,
                "[agent-input] force batch claim changed before dispatch"
            );
            Err("force batch changed before dispatch claim".into())
        }
        Err(error) => {
            tracing::error!(batch_id, error = %error, "[agent-input] force batch claim failed");
            Err(error.to_string())
        }
    }
}

async fn consume_force_batch(
    context: &WorkerContext<'_>,
    batch_id: &str,
    turn_generation: i64,
    expected_items: &[AgentInputItem],
) {
    let items = match agent_input_outbox_service::list_force_batch(&context.db.conn, batch_id).await
    {
        Ok(items) if !items.is_empty() => items,
        Ok(_) => {
            log_missing_batch(context, batch_id, turn_generation, expected_items).await;
            return;
        }
        Err(error) => {
            tracing::error!(batch_id, turn_generation, error = %error, "[agent-input] force batch could not be loaded for acceptance settlement");
            return;
        }
    };
    let result = agent_input_outbox_service::transition_force_batch(
        &context.db.conn,
        crate::db::service::agent_input_force_service::ForceBatchTransition {
            batch_id,
            turn_generation,
            from: AgentInputStatus::Dispatching,
            to: AgentInputStatus::Consumed,
            error: None,
        },
    )
    .await;
    match result {
        Ok(changed) if changed == items.len() as u64 => {
            for item in items {
                emit_current(context.db, context.state, context.emitter, &item.id).await;
            }
        }
        Ok(changed) => tracing::error!(
            batch_id,
            turn_generation,
            expected = items.len(),
            changed,
            "[agent-input] force batch acceptance settlement was partial"
        ),
        Err(error) => {
            tracing::error!(batch_id, turn_generation, error = %error, "[agent-input] force batch acceptance settlement failed")
        }
    }
}

async fn log_missing_batch(
    context: &WorkerContext<'_>,
    batch_id: &str,
    turn_generation: i64,
    expected_items: &[AgentInputItem],
) {
    if force_batch_already_consumed(context, expected_items, turn_generation).await {
        tracing::info!(
            batch_id,
            turn_generation,
            "[agent-input] force batch was already settled by turn completion"
        );
    } else {
        tracing::error!(
            batch_id,
            turn_generation,
            "[agent-input] force batch disappeared before acceptance settlement"
        );
    }
}

async fn force_batch_already_consumed(
    context: &WorkerContext<'_>,
    expected_items: &[AgentInputItem],
    turn_generation: i64,
) -> bool {
    for expected in expected_items {
        let Ok(Some(current)) =
            agent_input_outbox_service::get(&context.db.conn, &expected.id).await
        else {
            return false;
        };
        if current.status != AgentInputStatus::Consumed
            || current.strategy != Some(AgentInputStrategy::SafeForceNext)
            || current.target_turn_generation != Some(turn_generation)
        {
            return false;
        }
    }
    true
}

async fn fail_force_batch(context: &WorkerContext<'_>, batch_id: &str, error: String) {
    let items = agent_input_outbox_service::list_force_batch(&context.db.conn, batch_id)
        .await
        .unwrap_or_default();
    match agent_input_outbox_service::fail_force_batch(&context.db.conn, batch_id, error).await {
        Ok(_) => {
            for item in items {
                emit_current(context.db, context.state, context.emitter, &item.id).await;
            }
        }
        Err(error) => {
            tracing::error!(batch_id, error = %error, "[agent-input] force batch failure settlement failed")
        }
    }
}
