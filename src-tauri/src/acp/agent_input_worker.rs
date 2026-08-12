use std::sync::Arc;
use std::time::Duration;

use crate::acp::agent_input_capabilities::{feedback_text, AgentInputCapabilities};
use crate::acp::agent_input_dispatch::AgentInputRuntime;
use crate::acp::agent_input_worker_dispatch::{dispatch_feedback, dispatch_native, dispatch_next};
use crate::acp::agent_input_worker_force::dispatch_force_batch;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::{SessionState, ToolCallStatus};
use crate::acp::types::ConnectionStatus;
use crate::acp::{AgentInputItem, AgentInputStatus};
use crate::db::service::agent_input_outbox_service;
use crate::db::AppDatabase;
use crate::web::event_bridge::EventEmitter;

const OUTBOX_READ_RETRY_DELAY: Duration = Duration::from_secs(1);
pub(super) struct WorkerSnapshot {
    pub(super) conversation_id: i32,
    pub(super) folder_id: i32,
    turn_in_flight: bool,
    turn_completion_pending: bool,
    native_background_active: bool,
    pub(super) turn_generation: i64,
    capabilities: AgentInputCapabilities,
    has_tools: bool,
    has_running_tools: bool,
    wake: tokio::sync::futures::OwnedNotified,
}

pub(super) struct WorkerContext<'a> {
    pub(super) manager: &'a ConnectionManager,
    pub(super) db: &'a Arc<AppDatabase>,
    pub(super) state: &'a Arc<tokio::sync::RwLock<SessionState>>,
    pub(super) emitter: &'a EventEmitter,
    pub(super) conn_id: &'a str,
}

#[derive(Default)]
struct WorkerTracking {
    observed_tool_for: Option<String>,
    cancel_requested_for: Option<String>,
}

enum PendingWork {
    ForceBatch(Vec<AgentInputItem>),
    Item(AgentInputItem),
}

pub(crate) async fn run(
    runtime: &Arc<AgentInputRuntime>,
    manager: &ConnectionManager,
    conn_id: &str,
) {
    let Some(db) = runtime.db() else {
        return;
    };
    let mut tracking = WorkerTracking::default();
    loop {
        let Some((state, emitter)) = manager.get_state_and_emitter(conn_id).await else {
            return;
        };
        let Some(snapshot) = worker_snapshot(&state).await else {
            return;
        };
        let work = match load_work(&db, snapshot.conversation_id, conn_id).await {
            Ok(Some(work)) => work,
            Ok(None) => {
                // Keep the worker alive across an empty read. A submit can race
                // this query; its state-change notification wakes this wait.
                snapshot.wake.await;
                continue;
            }
            Err(()) => {
                tokio::time::sleep(OUTBOX_READ_RETRY_DELAY).await;
                continue;
            }
        };
        let context = WorkerContext {
            manager,
            db: &db,
            state: &state,
            emitter: &emitter,
            conn_id,
        };
        match work {
            PendingWork::ForceBatch(items) => {
                tracking
                    .process_force_batch(&context, items, snapshot)
                    .await
            }
            PendingWork::Item(item) => tracking.process(&context, item, snapshot).await,
        }
    }
}

async fn load_work(
    db: &Arc<AppDatabase>,
    conversation_id: i32,
    conn_id: &str,
) -> Result<Option<PendingWork>, ()> {
    match agent_input_outbox_service::active_force_batch(&db.conn, conversation_id).await {
        Ok(items) if !items.is_empty() => return Ok(Some(PendingWork::ForceBatch(items))),
        Ok(_) => {}
        Err(error) => {
            tracing::error!(connection_id = conn_id, error = %error, "[agent-input] force batch read failed");
            return Err(());
        }
    }
    match agent_input_outbox_service::next_unsettled(&db.conn, conversation_id).await {
        Ok(item) => Ok(item.map(PendingWork::Item)),
        Err(error) => {
            tracing::error!(connection_id = conn_id, error = %error, "[agent-input] outbox read failed");
            Err(())
        }
    }
}

async fn worker_snapshot(state: &Arc<tokio::sync::RwLock<SessionState>>) -> Option<WorkerSnapshot> {
    let snapshot = state.read().await;
    if matches!(
        snapshot.status,
        ConnectionStatus::Disconnected | ConnectionStatus::Error
    ) {
        return None;
    }
    let wake = Arc::clone(&snapshot.agent_input_notify).notified_owned();
    let has_tools = !snapshot.active_tool_calls.is_empty();
    let has_running_tools = snapshot.active_tool_calls.values().any(|tool| {
        matches!(
            tool.status,
            ToolCallStatus::Pending | ToolCallStatus::InProgress
        )
    });
    Some(WorkerSnapshot {
        conversation_id: snapshot.conversation_id?,
        folder_id: snapshot.folder_id?,
        turn_in_flight: snapshot.turn_in_flight,
        turn_completion_pending: snapshot.turn_completion_pending,
        native_background_active: snapshot.native_background_turn.is_some(),
        turn_generation: snapshot.turn_generation,
        capabilities: AgentInputCapabilities::for_connection(
            snapshot.agent_type,
            snapshot.feedback_tool_available,
            snapshot.native_steering_available,
        ),
        has_tools,
        has_running_tools,
        wake,
    })
}

impl WorkerTracking {
    async fn process(
        &mut self,
        context: &WorkerContext<'_>,
        item: AgentInputItem,
        snapshot: WorkerSnapshot,
    ) {
        self.reset(&item.id);
        if snapshot.native_background_active {
            snapshot.wake.await;
            return;
        }
        if item.status == AgentInputStatus::Dispatching {
            if snapshot.turn_in_flight || snapshot.turn_completion_pending {
                snapshot.wake.await;
            } else {
                context.recover_stale_dispatch(&item).await;
            }
            return;
        }
        if snapshot.turn_in_flight {
            self.process_in_flight(context, &item, snapshot).await;
            return;
        }
        if snapshot.turn_completion_pending {
            snapshot.wake.await;
            return;
        }
        self.clear();
        dispatch_next(context, item, snapshot).await;
    }

    async fn process_force_batch(
        &mut self,
        context: &WorkerContext<'_>,
        items: Vec<AgentInputItem>,
        snapshot: WorkerSnapshot,
    ) {
        let Some(batch_id) = items.first().and_then(|item| item.force_batch_id.clone()) else {
            return;
        };
        self.reset(&batch_id);
        if snapshot.native_background_active {
            snapshot.wake.await;
            return;
        }
        if snapshot.turn_in_flight {
            if snapshot.has_running_tools {
                self.observed_tool_for = Some(batch_id);
            } else if self.cancel_requested_for.as_deref() != Some(batch_id.as_str()) {
                match context
                    .manager
                    .request_safe_cancel(context.conn_id, snapshot.turn_generation)
                    .await
                {
                    Ok(()) => self.cancel_requested_for = Some(batch_id),
                    Err(error) => tracing::warn!(
                        connection_id = context.conn_id,
                        error = %error,
                        "[agent-input] force batch safe cancellation request failed"
                    ),
                }
            }
            snapshot.wake.await;
            return;
        }
        if snapshot.turn_completion_pending {
            snapshot.wake.await;
            return;
        }
        self.clear();
        dispatch_force_batch(context, items, snapshot).await;
    }
    fn reset(&mut self, id: &str) {
        if self
            .observed_tool_for
            .as_deref()
            .is_some_and(|value| value != id)
        {
            self.clear();
        }
    }

    fn clear(&mut self) {
        self.observed_tool_for = None;
        self.cancel_requested_for = None;
    }

    async fn process_in_flight(
        &mut self,
        context: &WorkerContext<'_>,
        item: &AgentInputItem,
        snapshot: WorkerSnapshot,
    ) {
        let native_already_attempted =
            item.strategy == Some(crate::acp::AgentInputStrategy::NativeSteer);
        if !native_already_attempted {
            if let Some(kind) = snapshot.capabilities.native_steer_for(&item.payload) {
                dispatch_native(context, item, &snapshot, kind).await;
                return;
            }
        }
        if (native_already_attempted
            || snapshot
                .capabilities
                .native_steer_for(&item.payload)
                .is_none())
            && snapshot.capabilities.supports_feedback(&item.payload)
            && feedback_text(&item.payload).is_some()
        {
            dispatch_feedback(context, item, &snapshot).await;
            return;
        }
        if snapshot.capabilities.uses_deferred_interrupt() {
            self.observe_safe_point(context, item, &snapshot).await;
        }
        snapshot.wake.await;
    }

    async fn observe_safe_point(
        &mut self,
        context: &WorkerContext<'_>,
        item: &AgentInputItem,
        snapshot: &WorkerSnapshot,
    ) {
        if snapshot.has_running_tools {
            self.observed_tool_for = Some(item.id.clone());
            return;
        }
        let reached = snapshot.has_tools
            && self.observed_tool_for.as_deref() == Some(item.id.as_str())
            && self.cancel_requested_for.as_deref() != Some(item.id.as_str());
        if reached
            && context
                .manager
                .request_safe_cancel(context.conn_id, snapshot.turn_generation)
                .await
                .is_ok()
        {
            self.cancel_requested_for = Some(item.id.clone());
        }
    }
}
