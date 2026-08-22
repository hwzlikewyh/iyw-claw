//! Background subscriber that watches the in-process `InternalEventBus` for
//! ACP events that need cross-connection DB persistence (e.g. binding the
//! agent's external session id onto a conversation row when SessionStarted
//! fires). Decoupled from `emit_with_state` so the emit hot path stays
//! lock-tight.
//!
//! Phase 5: migrated from `WebEventBroadcaster` (JSON-shape) to
//! `InternalEventBus` (typed `Arc<EventEnvelope>`). Eliminates the
//! per-event `serde_json::from_value` reparse and lets us drop the
//! `acp://event` channel from the global firehose entirely.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::sync::{broadcast, mpsc};

use crate::acp::delegation::broker::{
    delegation_ack_task_id, DelegationBroker, DelegationMatchKey,
};
use crate::acp::delegation::types::{DelegationError, DelegationOutcome, DelegationSuccess};
use crate::acp::internal_bus::InternalEventBus;
use crate::acp::manager::ConnectionManager;
use crate::acp::session_state::SessionState;
use crate::acp::types::{AcpEvent, ConnectionStatus, EventEnvelope};
use crate::browser::BrowserSessionManager;
use crate::chat_channel::manager::ChatChannelManager;
use crate::commands::conversation_title::{self, ConversationTitleContext};
use crate::db::entities::conversation::ConversationStatus;
use crate::db::error::DbError;
use crate::db::service::{automation_service, conversation_service};
use crate::models::AgentType;
use crate::user_memory::UserMemoryService;
use crate::web::event_bridge::{emit_with_state, EventEmitter};
use tokio::sync::RwLock;

/// Per-connection worker queue depth. Sized for the **filtered** event set
/// only (see `is_lifecycle_relevant`) — high-frequency events (ContentDelta,
/// ToolCall*, PermissionRequest) are dropped at the dispatcher and never
/// enter the queue. The remaining event types arrive at most a handful
/// of times per turn, so 64 slots is comfortable headroom for a sustained
/// SQLite stall without forcing the dispatcher to block on `send`.
const WORKER_QUEUE_CAPACITY: usize = 64;

/// Whether an event needs to reach the per-connection worker. Mirrors the
/// match arms in `connection_worker_loop` — keep in sync so the dispatcher
/// doesn't filter out an event a future worker arm starts caring about.
///
/// Filtering at the dispatcher (rather than letting the worker no-op on
/// uninteresting events) means ContentDelta floods can't crowd out a
/// TurnComplete in the worker mailbox: only events that may write the DB
/// or update the per-connection cache enter the queue.
///
/// `ToolCall`/`ToolCallUpdate` are deliberately NOT in the accept list.
/// Delegation correlation (capturing `delegate_to_agent` tool_call_ids for
/// the broker's pending queue) used to ride the worker's `ToolCall` arm, but
/// that coupled a latency-critical, lossless registration to the DB-stalling
/// worker AND fed every `ToolCall` (including each parallel child's tool
/// stream) into worker mailboxes — pressure that could block the dispatcher
/// and lag the bus into dropping a parent's second delegation `tool_call`.
/// Registration now happens synchronously in the dispatcher loop via
/// `register_delegation_tool_call_from_event`, so these high-frequency events
/// never need to reach a worker.
fn is_lifecycle_relevant(event: &AcpEvent) -> bool {
    matches!(
        event,
        AcpEvent::SessionStarted { .. }
            | AcpEvent::SessionTitleUpdated { .. }
            | AcpEvent::TurnComplete { .. }
            | AcpEvent::UserMessage { .. }
            | AcpEvent::ConversationLinked { .. }
            | AcpEvent::ConversationStatusChanged {
                status: ConversationStatus::Cancelled,
                ..
            }
            | AcpEvent::StatusChanged {
                status: ConnectionStatus::Disconnected
            }
            | AcpEvent::Error { .. }
    )
}

/// Whether the dispatcher should tear down (drop the sender for) the per-
/// connection worker after forwarding this event. Two cases:
///
///   - `Disconnected` — the normal teardown signal, always emitted by
///     `connection.rs` after `run_connection` returns.
///   - `Error { terminal: true }` — defense-in-depth for the case where
///     the bus drops the trailing `Disconnected` (`Lagged`) or the
///     `run_connection` task aborts between emit sites. The worker
///     dispatches terminal work on whichever lands first (P1); without
///     also dropping the sender here, a missed `Disconnected` would leak
///     the worker task + its `CachedConn` for the lifetime of the process.
///
/// Non-terminal `Error` is NOT terminal at the dispatcher level — it also
/// fires mid-turn from `turn_failure_error_event` while the child connection
/// stays alive, and the worker must survive to process the trailing
/// `TurnComplete`. (P2 follow-up in the v0.14.3 post-mortem review.)
fn is_dispatcher_terminal(event: &AcpEvent) -> bool {
    matches!(
        event,
        AcpEvent::StatusChanged {
            status: ConnectionStatus::Disconnected
        } | AcpEvent::Error { terminal: true, .. }
    )
}

/// Per-connection state that survives `ConnectionCleanupGuard::drop` so
/// `Disconnected` / `Error` handlers can still emit a derived
/// `ConversationStatusChanged` after the manager entry has been removed.
///
/// Captured on `ConversationLinked` (the earliest point a connection is bound
/// to a conversation row) and consulted on terminal status events. Without
/// this cache, `manager.get_state_and_emitter(connection_id)` races the
/// cleanup guard: `emit_with_state(StatusChanged{Disconnected})` writes to the
/// broadcaster *before* the guard drops, but the subscriber's async receive
/// can wake up after the entry is already gone.
struct CachedConn {
    conversation_id: i32,
    state: Arc<RwLock<SessionState>>,
    emitter: EventEmitter,
}

/// Backoff schedule for `handle_event` DB writes. Most transient
/// SQLite contention clears within the first retry; the third gives a
/// final chance before we fall back to "log loudly and move on".
const HANDLE_EVENT_RETRY_BACKOFFS: &[Duration] =
    &[Duration::from_millis(100), Duration::from_millis(500)];

/// Wrap `handle_event` with a small backoff retry. Most failures here
/// are transient SQLite "database is locked" errors that clear within a
/// few hundred milliseconds; without a retry the conversation row would
/// silently miss its `pending_review` write and the sidebar would stay
/// stuck on `in_progress` until the next prompt's `in_progress` write.
///
/// Final failure is logged at ERROR — this is the only signal the
/// subscriber is dropping correctness on the floor, so it must be noisy.
async fn handle_event_with_retry(
    db_conn: &DatabaseConnection,
    manager: &ConnectionManager,
    chat_channel_manager: &ChatChannelManager,
    envelope: &EventEnvelope,
    broker: Option<&Arc<DelegationBroker>>,
    harvest_service: Option<&Arc<UserMemoryService>>,
) {
    let event_kind = lifecycle_event_kind(&envelope.payload);
    match handle_event(
        db_conn,
        manager,
        chat_channel_manager,
        envelope,
        broker,
        harvest_service,
    )
    .await
    {
        Ok(()) => return,
        Err(e) => {
            tracing::warn!(
                connection_id = %envelope.connection_id,
                event_kind,
                error = %e,
                "[lifecycle][WARN] handle_event failed (attempt 1, will retry)"
            );
        }
    }
    for (attempt, backoff) in HANDLE_EVENT_RETRY_BACKOFFS.iter().enumerate() {
        tokio::time::sleep(*backoff).await;
        match handle_event(
            db_conn,
            manager,
            chat_channel_manager,
            envelope,
            broker,
            harvest_service,
        )
        .await
        {
            Ok(()) => return,
            Err(e) => {
                let attempt_num = attempt + 2;
                let is_last = attempt + 1 == HANDLE_EVENT_RETRY_BACKOFFS.len();
                let level = if is_last { "ERROR" } else { "WARN" };
                tracing::warn!(
                    connection_id = %envelope.connection_id,
                    event_kind,
                    error = %e,
                    attempt = attempt_num,
                    final_attempt = is_last,
                    "[lifecycle][{level}] handle_event failed"
                );
            }
        }
    }
}

fn lifecycle_event_kind(event: &AcpEvent) -> &'static str {
    match event {
        AcpEvent::SessionStarted { .. } => "session_started",
        AcpEvent::SessionTitleUpdated { .. } => "session_title_updated",
        AcpEvent::TurnComplete { .. } => "turn_complete",
        AcpEvent::UserMessage { .. } => "user_message",
        AcpEvent::ConversationLinked { .. } => "conversation_linked",
        AcpEvent::ConversationStatusChanged { .. } => "conversation_status_changed",
        AcpEvent::StatusChanged { .. } => "status_changed",
        AcpEvent::Error { .. } => "error",
        _ => "other",
    }
}

async fn refresh_agent_title(
    db_conn: &DatabaseConnection,
    manager: &ConnectionManager,
    chat_channel_manager: &ChatChannelManager,
    connection_id: &str,
    title: &str,
) -> Result<(), DbError> {
    let Some((state, emitter)) = manager.get_state_and_emitter(connection_id).await else {
        return Ok(());
    };
    let Some(conversation_id) = state.read().await.conversation_id else {
        return Ok(());
    };
    let summary = conversation_service::get_by_id(db_conn, conversation_id).await?;
    let title_context = ConversationTitleContext {
        conn: db_conn,
        emitter: &emitter,
        chat_channel_manager,
    };
    let fallback_title = crate::parsers::title_from_user_text(title);
    let is_fallback = summary.title_source
        == crate::db::entities::conversation::ConversationTitleSource::UserFallback
        && summary.title.as_deref() == Some(fallback_title.as_str());
    let changed = if is_fallback {
        conversation_title::refresh_fallback(&title_context, conversation_id, &fallback_title)
            .await?
    } else {
        conversation_title::refresh_auto(&title_context, conversation_id, title).await?
    };
    if !changed {
        return Ok(());
    }
    tracing::info!(
        connection_id = %connection_id,
        conversation_id,
        title_chars = title.chars().count(),
        fallback = is_fallback,
        "[lifecycle] Agent session title applied"
    );
    Ok(())
}

pub(crate) async fn handle_event(
    db_conn: &DatabaseConnection,
    manager: &ConnectionManager,
    chat_channel_manager: &ChatChannelManager,
    envelope: &EventEnvelope,
    broker: Option<&Arc<DelegationBroker>>,
    harvest_service: Option<&Arc<UserMemoryService>>,
) -> Result<(), DbError> {
    match &envelope.payload {
        // NOTE: parent-side `delegate_to_agent` tool_call_id capture used to
        // live here (a `ToolCall` arm). It now runs in the dispatcher loop via
        // `register_delegation_tool_call_from_event`, off the DB-coupled worker
        // and across both `ToolCall` and `ToolCallUpdate`, so `ToolCall` no
        // longer reaches this worker at all (see `is_lifecycle_relevant`).
        AcpEvent::SessionTitleUpdated { title } => {
            refresh_agent_title(
                db_conn,
                manager,
                chat_channel_manager,
                &envelope.connection_id,
                title,
            )
            .await
        }
        AcpEvent::ConversationLinked { .. } => {
            let title = match manager.get_state(&envelope.connection_id).await {
                Some(state) => state
                    .read()
                    .await
                    .agent_title_candidate
                    .as_ref()
                    .filter(|candidate| candidate.event_seq < envelope.seq)
                    .map(|candidate| candidate.title.clone()),
                None => None,
            };
            let Some(title) = title else {
                return Ok(());
            };
            refresh_agent_title(
                db_conn,
                manager,
                chat_channel_manager,
                &envelope.connection_id,
                &title,
            )
            .await
        }
        AcpEvent::SessionStarted { session_id } => {
            // Look up conversation_id (and the emitter) from the live state.
            let Some((state_arc, emitter)) =
                manager.get_state_and_emitter(&envelope.connection_id).await
            else {
                return Ok(());
            };
            let (conversation_id, transition, channel_owned) = {
                let state = state_arc.read().await;
                (
                    state.conversation_id,
                    state.session_started_transition(envelope.seq).cloned(),
                    state.owner_window_label.starts_with("chat_channel:"),
                )
            };
            let Some(transition) = transition else {
                tracing::warn!(
                    connection_id = %envelope.connection_id,
                    event_seq = envelope.seq,
                    session_id,
                    "[lifecycle] SessionStarted transition was evicted before persistence"
                );
                return Ok(());
            };
            // Channel sessions commit the external id and durable route in one
            // transaction in `session_event_subscriber`.
            if channel_owned {
                return Ok(());
            }
            if let Some(cid) = conversation_id {
                let updated = conversation_service::update_external_id_if_matches(
                    db_conn,
                    cid,
                    transition.expected_external_id.as_deref(),
                    session_id,
                )
                .await?;
                if !updated {
                    tracing::warn!(
                        connection_id = %envelope.connection_id,
                        conversation_id = cid,
                        expected_external_id = transition
                            .expected_external_id
                            .as_deref()
                            .unwrap_or(""),
                        session_id,
                        "[lifecycle] ignored stale SessionStarted persistence"
                    );
                    return Ok(());
                }
                // The external_id just landed on the row. The create-time
                // sidebar upsert carried `external_id: null` (no session yet),
                // so re-broadcast the full summary on `conversation://changed`
                // to converge every client. Root-only (the helper skips
                // delegation children). Best-effort, after the DB write.
                crate::commands::conversations::emit_conversation_upsert(&emitter, db_conn, cid)
                    .await;
            }
            Ok(())
        }
        AcpEvent::UserMessage { message_id, .. } => {
            let Some((state, emitter)) =
                manager.get_state_and_emitter(&envelope.connection_id).await
            else {
                return Ok(());
            };
            crate::acp::agent_input_lifecycle::consume_user_message(
                db_conn, &state, &emitter, message_id,
            )
            .await
        }
        AcpEvent::TurnComplete { stop_reason, .. } => {
            // Centralized status transition: when the agent reports the turn
            // is done, flip the conversation row and re-broadcast the change
            // as `ConversationStatusChanged`. This lives in the lifecycle
            // subscriber (rather than at the original emit site in
            // `acp/connection.rs`) so the write is decoupled from the
            // protocol-event hot path AND survives a frontend refresh
            // mid-turn — the row gets the correct status even if no
            // browser is connected to react to TurnComplete itself.
            //
            // The target status depends on the stop reason: `end_turn` is the
            // only success case and goes to `PendingReview`. `refusal`,
            // `max_tokens`, `max_turn_requests`, `unknown`, and `empty`
            // indicate the turn failed (often a backend/gateway error
            // masquerading as `Refusal` per the ACP spec gap, or — common
            // with OpenCode — a silent EndTurn that produced no output), so
            // we flip to `Cancelled` and pair the transition with an
            // `AcpEvent::Error` toast emitted upstream by `connection.rs`.
            // `cancelled` is already written by `manager.cancel()` (eager
            // CAS InProgress → Cancelled at the user-cancel entry point), so
            // we leave it alone here. `completed` transitions remain
            // frontend-driven.
            let target_status = match stop_reason.as_str() {
                "end_turn" => Some(ConversationStatus::PendingReview),
                "refusal" | "max_tokens" | "max_turn_requests" | "unknown" | "empty" => {
                    Some(ConversationStatus::Cancelled)
                }
                // `cancelled` and any future reason: don't write here.
                _ => None,
            };
            let Some((state_arc, emitter)) =
                manager.get_state_and_emitter(&envelope.connection_id).await
            else {
                return Ok(());
            };
            let (conversation_id, last_text, current_model, turn_generation, title_input) = {
                let mut snap = state_arc.write().await;
                (
                    snap.conversation_id,
                    snap.last_assistant_text.clone(),
                    snap.current_model.clone(),
                    snap.turn_generation,
                    snap.last_completed_turn_title_input.take(),
                )
            };
            // No conversation row bound (defensive — should never happen in
            // practice since `send_prompt_linked` runs before TurnComplete can
            // fire). Nothing to update.
            let Some(cid) = conversation_id else {
                manager
                    .finish_agent_input_turn_settlement(&envelope.connection_id, turn_generation)
                    .await;
                return Ok(());
            };
            let mut completion_error = None;
            if let Err(error) =
                automation_service::record_stop_reason(db_conn, cid, stop_reason).await
            {
                completion_error = Some(error);
            }
            if let Some(ts) = target_status.clone() {
                // DB write before emit so any downstream subscriber that observes
                // the ConversationStatusChanged event can assume the row is
                // already at the target status.
                match conversation_service::update_status(db_conn, cid, ts.clone()).await {
                    Ok(()) => {
                        emit_with_state(
                            &state_arc,
                            &emitter,
                            AcpEvent::ConversationStatusChanged {
                                conversation_id: cid,
                                status: ts,
                            },
                        )
                        .await;
                    }
                    Err(error) => {
                        if completion_error.is_none() {
                            completion_error = Some(error);
                        } else {
                            tracing::error!(
                                conversation_id = cid,
                                error = %error,
                                "[lifecycle] conversation status write also failed"
                            );
                        }
                    }
                }
            }
            if stop_reason == "end_turn" {
                crate::acp::task_artifact_delivery::deliver_completed_turn(
                    crate::acp::task_artifact_delivery::CompletedTurnDelivery {
                        db: db_conn,
                        state: &state_arc,
                        emitter: &emitter,
                        connection_id: &envelope.connection_id,
                        conversation_id: cid,
                        turn_generation,
                    },
                )
                .await;
                if let Some(input) = title_input {
                    let title_context = ConversationTitleContext {
                        conn: db_conn,
                        emitter: &emitter,
                        chat_channel_manager,
                    };
                    if let Err(error) =
                        crate::acp::conversation_title_summary::schedule_first_turn_summary(
                            &title_context,
                            cid,
                            input,
                        )
                        .await
                    {
                        tracing::warn!(
                            conversation_id = cid,
                            error = %error,
                            "[conversation-title] summary scheduling failed"
                        );
                    }
                }
            }

            // If this conversation was spawned by a delegation, resolve the
            // pending broker call. The broker maps the outcome onto the
            // parent's `tool_use_id` via the registered `call_id`.
            if let Some(b) = broker {
                forward_turn_complete_to_broker(
                    db_conn,
                    b.as_ref(),
                    cid,
                    stop_reason.as_str(),
                    last_text,
                )
                .await;
            }

            // Persist the model name that was active when this turn finished.
            // Best-effort: a None current_model (agent has no model selector)
            // clears the field, which is the desired initial state.
            if let Err(e) = conversation_service::update_model(db_conn, cid, current_model).await {
                tracing::warn!("[lifecycle] failed to persist model for conversation {cid}: {e}");
            }

            // Task 13: enqueue the completed turn into the user-memory harvest
            // queue. Best-effort — failures only log and must never block the
            // completion event. The capture was taken at the emit site (before
            // `MemoryTurnTracker::complete_turn` cleared the active bit).
            if let Some(harvest) = harvest_service {
                let capture = {
                    let mut snap = state_arc.write().await;
                    snap.last_completed_turn_harvest.take()
                };
                if let Some(capture) = capture {
                    let agent_type = state_arc.read().await.agent_type.clone();
                    let request = crate::user_memory::MemoryHarvestRequest {
                        conversation: cid.to_string(),
                        turn_nonce: capture.turn_nonce,
                        agent_type,
                        stop_reason: Some(capture.stop_reason),
                        user_input_ref: capture.user_input_ref,
                        assistant_input_ref: capture.assistant_input_ref,
                        submitted_at: chrono::Utc::now().to_rfc3339(),
                    };
                    if let Err(error) = harvest.submit_turn_harvest(request).await {
                        tracing::warn!(
                            "[lifecycle] user-memory harvest submit failed for conversation {cid}: {error}"
                        );
                    }
                }
            }
            if let Err(error) = crate::acp::agent_input_lifecycle::fallback_unconsumed_turn(
                db_conn,
                manager,
                &state_arc,
                &emitter,
                &envelope.connection_id,
                turn_generation,
            )
            .await
            {
                if completion_error.is_none() {
                    completion_error = Some(error);
                } else {
                    tracing::error!(
                        connection_id = %envelope.connection_id,
                        turn_generation,
                        error = %error,
                        "[agent-input] fallback settlement also failed"
                    );
                }
            }
            manager
                .finish_agent_input_turn_settlement(&envelope.connection_id, turn_generation)
                .await;
            completion_error.map_or(Ok(()), Err)
        }
        // Other events don't need cross-connection DB persistence today; extend
        // this dispatcher with new arms as the lifecycle scope grows.
        _ => Ok(()),
    }
}

/// On TurnComplete for a delegation child, resolve the pending broker call
/// and let the broker drive the rest of the lifecycle (meta write, the
/// `AcpEvent::DelegationCompleted` emit against the parent stream, child
/// disconnect, tx.send). Keeping the emit responsibility inside
/// `broker.complete_call` is what guarantees the broker's other terminal
/// paths (`timeout` / `cancel_by_child_connection` / `cancel_by_parent`)
/// also surface the event — see
/// `.docs/issues/2026-05-24-delegation-termination-cascade.md`.
async fn forward_turn_complete_to_broker(
    db_conn: &DatabaseConnection,
    broker: &DelegationBroker,
    conversation_id: i32,
    stop_reason: &str,
    last_text: Option<String>,
) {
    let row = match conversation_service::get_by_id(db_conn, conversation_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                "[delegation][lifecycle] couldn't fetch child conversation \
                 {conversation_id} for outcome routing: {e}"
            );
            return;
        }
    };
    let call_id = match row.delegation_call_id.clone() {
        Some(id) => id,
        None => return, // not a delegation child; nothing to do.
    };
    if row.parent_tool_use_id.is_none() {
        tracing::info!(
            "[delegation][lifecycle] conversation {conversation_id} has \
             delegation_call_id but no parent_tool_use_id; dropping"
        );
        return;
    }
    let agent_type = row.agent_type;
    let outcome = match stop_reason {
        "end_turn" => DelegationOutcome::Ok(DelegationSuccess {
            text: last_text.unwrap_or_default(),
            child_conversation_id: conversation_id,
            child_agent_type: agent_type,
            turn_count: 1,
            duration_ms: 0,
            token_usage: None,
        }),
        "cancelled" => DelegationOutcome::from_err(
            DelegationError::Canceled {
                reason: "child session was cancelled".into(),
            },
            Some(conversation_id),
        ),
        // Each child turn-failure reason gets a distinct wire code so the
        // parent UI can show a more useful error label than a generic
        // "subagent error". Mirrors the parent's own
        // `turn_failure_error_event` mapping in `connection.rs`.
        "refusal" => {
            DelegationOutcome::from_err(DelegationError::ChildRefusal, Some(conversation_id))
        }
        "max_tokens" => {
            DelegationOutcome::from_err(DelegationError::ChildMaxTokens, Some(conversation_id))
        }
        "max_turn_requests" => DelegationOutcome::from_err(
            DelegationError::ChildMaxTurnRequests,
            Some(conversation_id),
        ),
        "empty" => DelegationOutcome::from_err(DelegationError::ChildEmpty, Some(conversation_id)),
        other => DelegationOutcome::from_err(
            DelegationError::ChildUnknown(other.to_string()),
            Some(conversation_id),
        ),
    };
    broker.complete_call(&call_id, outcome).await;
}

/// Snapshot the connection's `(state, emitter)` into the lifecycle cache when
/// `ConversationLinked` arrives. Idempotent on repeat calls (re-link on the
/// already-bound path is a no-op so we don't churn the cached refs).
async fn try_cache_link(
    cache: &mut HashMap<String, CachedConn>,
    manager: &ConnectionManager,
    connection_id: &str,
    conversation_id: i32,
) {
    if cache.contains_key(connection_id) {
        return;
    }
    // The connection is necessarily still in the manager at this point —
    // `ConversationLinked` is emitted by `send_prompt_linked` from the
    // connection's own send path, well before any disconnect.
    let Some((state, emitter)) = manager.get_state_and_emitter(connection_id).await else {
        tracing::warn!(
            "[lifecycle][WARN] ConversationLinked for unknown connection {connection_id}; \
             skipping cache (terminal-status hand-off will no-op)"
        );
        return;
    };
    cache.insert(
        connection_id.to_string(),
        CachedConn {
            conversation_id,
            state,
            emitter,
        },
    );
}

/// Handle `StatusChanged{Disconnected}` / `Error` for a cached connection:
/// CAS the row from `InProgress` → `Cancelled` (preserves any prior
/// `PendingReview` from `TurnComplete` and any user-driven `Completed`),
/// re-emit `ConversationStatusChanged` if the write took effect.
///
/// Removing the cache entry on first terminal event handles the
/// `Error` → `Disconnected` sequence that `connection.rs` emits on the error
/// path: the second event finds an empty cache and is a clean no-op, so we
/// don't pay a redundant DB read.
async fn handle_terminal_event(
    db_conn: &DatabaseConnection,
    cache: &mut HashMap<String, CachedConn>,
    connection_id: &str,
) -> Result<(), DbError> {
    let Some(entry) = cache.remove(connection_id) else {
        return Ok(());
    };
    let cid = entry.conversation_id;
    let changed = conversation_service::update_status_if(
        db_conn,
        cid,
        ConversationStatus::InProgress,
        ConversationStatus::Cancelled,
    )
    .await?;
    if !changed {
        return Ok(());
    }
    emit_with_state(
        &entry.state,
        &entry.emitter,
        AcpEvent::ConversationStatusChanged {
            conversation_id: cid,
            status: ConversationStatus::Cancelled,
        },
    )
    .await;
    Ok(())
}

/// On a non-TurnComplete terminal event (Disconnected / Error) for a
/// delegation child, surface a `canceled` outcome to the broker. The
/// child's DB row may already be marked `Cancelled` by `handle_terminal_event`
/// above; this separately wakes the parent's pending `delegate_to_agent`
/// tool_use_id. Match-by-`child_connection_id` is O(pending), bounded by
/// active delegations.
///
/// `terminal_error` is the formatted `AcpEvent::Error` detail (when the
/// caller arrived via `Error` rather than a bare `Disconnected`). It gets
/// stitched into the broker's canceled reason so the parent's
/// `delegate_to_agent` tool-call result surfaces the real failure cause.
async fn forward_disconnect_to_broker(
    broker: &DelegationBroker,
    connection_id: &str,
    terminal_error: Option<&str>,
) {
    broker
        .cancel_by_child_connection(connection_id, terminal_error)
        .await;
}

/// Build a single-line detail string from an `AcpEvent::Error` payload,
/// preferring the form `"[code] message"` when a stable code is present
/// (so the parent agent sees both the machine-readable bucket and the
/// human-readable text). Trims trailing whitespace; returns `message`
/// verbatim when no code is provided.
fn format_terminal_error(message: &str, code: Option<&str>) -> String {
    let trimmed = message.trim();
    match code {
        Some(c) if !c.trim().is_empty() => format!("[{c}] {trimmed}"),
        _ => trimmed.to_string(),
    }
}

/// Wrapper keys hosts use to nest the real tool arguments. JSON-RPC servers
/// and MCP relays pack the call as `{name, arguments}` or `{params: {...}}`;
/// some agents stash the args under a generic `input`/`payload` next to
/// `_meta`. Mirrors the frontend `ARGS_WRAPPER_KEYS` in
/// `delegated-sub-thread.tsx` so the two sides peel exactly the same shapes.
const ARGS_WRAPPER_KEYS: [&str; 5] = ["arguments", "input", "params", "payload", "_meta"];

/// Walk wrapper layers — and one level of double-encoded JSON-of-JSON — down to
/// the object that actually carries the `delegate_to_agent` arguments, and
/// return a clone of it. A node qualifies the moment it exposes any of
/// `task`/`agent_type`/`working_dir` as a string; otherwise we descend into the
/// known wrapper keys (depth-capped so pathological nesting can't loop).
///
/// Direct port of the frontend `findDelegationArgs` (`delegated-sub-thread.tsx`):
/// same wrapper keys, same depth-4 cap, same "first object with a delegation
/// field wins" rule. Keeping the walkers symmetric means a `raw_input` the card
/// can render into a task line is the same `raw_input` the broker can build a
/// correlation key from — so a host that wraps its ACP tool-call args (e.g.
/// Codex packs them under `params.input`; some relays double-encode the blob)
/// still gets a *keyed* pending entry instead of silently degrading to
/// FIFO/synthetic correlation, which is the exact failure the keyed-retention
/// fix exists to prevent.
fn find_delegation_args(
    value: &serde_json::Value,
    depth: u8,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    if depth > 4 {
        return None;
    }
    // Double-encoded: some hosts ship `raw_input` as a JSON string whose
    // contents are themselves the arg blob. Parse one inner layer and recurse.
    if let Some(s) = value.as_str() {
        let inner: serde_json::Value = serde_json::from_str(s).ok()?;
        return find_delegation_args(&inner, depth + 1);
    }
    let obj = value.as_object()?;
    // Direct hit: this object declares a delegation field at its top level.
    if obj.get("task").and_then(|v| v.as_str()).is_some()
        || obj.get("agent_type").and_then(|v| v.as_str()).is_some()
        || obj.get("working_dir").and_then(|v| v.as_str()).is_some()
    {
        return Some(obj.clone());
    }
    // Otherwise peel a known wrapper layer.
    for key in ARGS_WRAPPER_KEYS {
        if let Some(child) = obj.get(key) {
            if let Some(found) = find_delegation_args(child, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

/// True when the ACP `tool_call` smells like an invocation of the
/// `delegate_to_agent` MCP tool. Defensive on both inputs because the host
/// agent gets to decide both fields:
///
/// * `title` is a free-form human-readable string the host composes. Some
///   hosts copy the MCP method verbatim (`mcp__iyw-claw-mcp__delegate_to_agent`),
///   some prefix it with a verb (`Run mcp__…__delegate_to_agent`), some
///   rephrase it (`Delegate to codex`). We match by substring so any
///   form containing `delegate_to_agent` is captured.
/// * `raw_input` is the JSON arg blob the agent sent to the MCP server. The
///   `delegate_to_agent` schema requires `agent_type` AND `task`; presence
///   of both — after peeling any wrapper layers via [`find_delegation_args`] —
///   is a near-zero false-positive shape check that catches any host that
///   mangles the title beyond recognition, including ones that wrap their
///   tool-call args.
fn title_identifies_delegation(title: &str) -> bool {
    let normalized_title = title.to_ascii_lowercase().replace([' ', '-'], "_");
    normalized_title.contains("delegate_to_agent")
}

fn is_delegation_invocation(title: &str, raw_input: Option<&str>) -> bool {
    if title_identifies_delegation(title) {
        return true;
    }
    if let Some(raw) = raw_input {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
            if let Some(args) = find_delegation_args(&v, 0) {
                let has_task = args.get("task").and_then(|t| t.as_str()).is_some();
                let has_agent_type = args.get("agent_type").and_then(|a| a.as_str()).is_some();
                if has_task && has_agent_type {
                    return true;
                }
            }
        }
    }
    false
}

/// Build the broker's `(agent_type, task, working_dir)` correlation key from
/// a `delegate_to_agent` tool_call's `raw_input` JSON. All three are values
/// the LLM passed identically to the ACP tool call and the MCP `tools/call`,
/// so the triple uniquely identifies the call even when several
/// `delegate_to_agent` invocations are in flight at once (and, unlike `task`
/// alone, doesn't collide when two parallel calls target different agents —
/// or different directories — with the same task text). `working_dir` is the
/// LLM's explicit value (`None` when omitted), matching the broker's
/// `DelegationRequest::requested_working_dir`. The args are located via
/// [`find_delegation_args`], so hosts that wrap or double-encode `raw_input`
/// are keyed identically to hosts that send the fields at the top level.
/// Returns `None` when `raw_input` is absent, not JSON, has no locatable
/// delegation object, or is missing/unparseable for `agent_type`/`task` — the
/// broker then falls back to FIFO ordering.
fn extract_delegation_match_key(raw_input: Option<&str>) -> Option<DelegationMatchKey> {
    let raw = raw_input?;
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let args = find_delegation_args(&parsed, 0)?;
    let task = args.get("task").and_then(|v| v.as_str())?.to_string();
    // Parse `agent_type` through the same serde path the MCP listener uses,
    // so the stored enum equals `DelegationRequest::agent_type`.
    let agent_type: AgentType = serde_json::from_value(args.get("agent_type")?.clone()).ok()?;
    let working_dir = args
        .get("working_dir")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some(DelegationMatchKey {
        agent_type,
        task,
        working_dir,
    })
}

/// True when an ACP `ToolCallUpdate.status` string is terminal for delegation
/// correlation. The live value is `format!("{:?}", ToolCallStatus).to_lowercase()`
/// over the `agent-client-protocol-schema` enum (variants `Pending`,
/// `InProgress`, `Completed`, `Failed`), so terminal == `completed` | `failed`.
/// Cancellation never arrives via this field — it flows through the turn-cancel
/// / teardown path, which already drains pending entries on the broker. The
/// enum is `#[non_exhaustive]`; if a `Cancelled` variant is added upstream,
/// extend this set alongside `acp::connection`'s status mapping.
fn is_terminal_tool_call_status(status: Option<&str>) -> bool {
    matches!(status, Some("completed" | "failed"))
}

struct TerminalToolCall<'a> {
    tool_call_id: &'a str,
    content: Option<&'a str>,
    raw_output: Option<&'a str>,
    status: &'a str,
}

fn terminal_tool_call_fields(event: &AcpEvent) -> Option<TerminalToolCall<'_>> {
    match event {
        AcpEvent::ToolCall {
            tool_call_id,
            status,
            content,
            raw_output,
            ..
        } if is_terminal_tool_call_status(Some(status)) => Some(TerminalToolCall {
            tool_call_id,
            content: content.as_deref(),
            raw_output: raw_output.as_deref(),
            status,
        }),
        AcpEvent::ToolCallUpdate {
            tool_call_id,
            status,
            content,
            raw_output,
            ..
        } if is_terminal_tool_call_status(status.as_deref()) => Some(TerminalToolCall {
            tool_call_id,
            content: content.as_deref(),
            raw_output: raw_output.as_deref(),
            status: status.as_deref().unwrap_or_default(),
        }),
        _ => None,
    }
}

/// Synchronously register a parent-side `delegate_to_agent` tool_call_id with
/// the broker, straight off the in-process bus — i.e. NOT via the
/// per-connection worker.
///
/// Called from the dispatcher loop for BOTH `ToolCall` and `ToolCallUpdate`
/// so correlation is robust against the two failure modes that orphaned the
/// second of two parallel delegations to a synthetic id (dead "view session"
/// + stuck "sub-agent running…"):
///
/// 1. **Args arriving late.** Some hosts emit an arg-less initial `ToolCall`
///    (a model-generated `title` that doesn't contain `delegate_to_agent`,
///    `raw_input` still empty) and only ship the `agent_type`/`task` arguments
///    on a following `ToolCallUpdate`. The old code registered solely from the
///    initial `ToolCall` and filtered `ToolCallUpdate` out entirely, so such a
///    call was never registered and its MCP round-trip fell back to a
///    synthetic `delegation-<uuid>`. Handling both variants here registers (or
///    backfills the key onto) the id whenever the args first appear.
/// 2. **Bus lag / worker stall.** Registration used to run inside the
///    DB-coupled per-connection worker. Under the load two parallel children
///    create (each streaming many `ToolCall`s), a worker stalling on a SQLite
///    retry could fill its mailbox, block the dispatcher's `send().await`, and
///    let the broadcast bus lag — dropping the parent's *second* `tool_call`
///    before it was ever registered. Registering here, before the
///    `is_lifecycle_relevant` filter and any worker send, removes that
///    dependency; and because `ToolCall` is no longer forwarded to workers at
///    all, the very mailbox pressure that caused the lag is gone too.
///
/// Cheap on the hot path: the discriminant match plus `is_delegation_invocation`
/// (a substring test on `title`, and a JSON parse only when `raw_input` is
/// present) fast-rejects the high-frequency non-delegation `ToolCallUpdate`
/// flood — those carry streaming `raw_output`, not `raw_input`. The broker's
/// own two-tier dedupe absorbs the repeated registrations a multi-update
/// delegation call produces.
///
/// A TERMINAL tool-call event (status `completed`/`failed`, via EITHER
/// `ToolCall` or `ToolCallUpdate` — some hosts ship status flips on the
/// non-update variant) is resolved atomically by the broker. A complete running
/// ack in `content` / `raw_output` may claim only the same parent's binding whose
/// `call_id` exactly equals its validated UUID `task_id`; title, args, FIFO, and
/// arrival order never select a terminal late binding. Otherwise the real id is
/// only tombstoned when it was already pending, and is never newly queued.
async fn register_delegation_tool_call_from_event(
    broker: &DelegationBroker,
    envelope: &EventEnvelope,
) {
    if let Some(tool_call) = terminal_tool_call_fields(&envelope.payload) {
        resolve_terminal_delegation_tool_call(broker, &envelope.connection_id, tool_call).await;
        return;
    }
    register_nonterminal_delegation_tool_call(broker, envelope).await;
}

async fn resolve_terminal_delegation_tool_call(
    broker: &DelegationBroker,
    parent_connection_id: &str,
    tool_call: TerminalToolCall<'_>,
) {
    let task_id = delegation_ack_task_id(tool_call.content, tool_call.raw_output);
    let tombstoned = broker
        .resolve_terminal_tool_call_by_task_id(
            parent_connection_id,
            tool_call.tool_call_id,
            task_id.as_deref(),
        )
        .await;
    if tombstoned {
        let tool_call_id = tool_call.tool_call_id;
        let status = tool_call.status;
        tracing::info!(
            "[delegation] tombstoned stale parent tool_call_id={tool_call_id} on conn={parent_connection_id} (terminal status={status})"
        );
    }
}

async fn register_nonterminal_delegation_tool_call(
    broker: &DelegationBroker,
    envelope: &EventEnvelope,
) {
    let (tool_call_id, title, raw_input): (&String, &str, Option<&str>) = match &envelope.payload {
        AcpEvent::ToolCall {
            tool_call_id,
            title,
            raw_input,
            ..
        } => (tool_call_id, title.as_str(), raw_input.as_deref()),
        AcpEvent::ToolCallUpdate {
            tool_call_id,
            title,
            raw_input,
            ..
        } => (
            tool_call_id,
            title.as_deref().unwrap_or(""),
            raw_input.as_deref(),
        ),
        _ => return,
    };
    if !is_delegation_invocation(title, raw_input) {
        return;
    }
    let match_key = extract_delegation_match_key(raw_input);
    tracing::info!(
        "[delegation] registering parent tool_call_id={tool_call_id} on conn={} (keyed={})",
        envelope.connection_id,
        match_key.is_some()
    );
    broker
        .register_pending_tool_call_with_key(
            &envelope.connection_id,
            tool_call_id.clone(),
            match_key,
        )
        .await;
}

/// Per-connection worker that owns the cache for one connection and
/// serializes its DB writes. Multiple connections run in parallel; within a
/// connection, ordering is preserved by the mpsc FIFO. Decouples the bus
/// receiver from DB-write latency — a slow SQLite write on connection A no
/// longer blocks events for connection B from being drained off the
/// broadcast buffer (the prior failure mode that pushed `lagged_count`).
async fn connection_worker_loop(
    connection_id: String,
    db: DatabaseConnection,
    manager: ConnectionManager,
    chat_channel_manager: ChatChannelManager,
    broker: Option<Arc<DelegationBroker>>,
    harvest_service: Option<Arc<UserMemoryService>>,
    browser: Option<BrowserSessionManager>,
    mut rx: mpsc::Receiver<Arc<EventEnvelope>>,
) {
    // 1-entry HashMap so we can reuse `handle_terminal_event` (also keeps the
    // existing test surface intact — tests still drive a `&mut HashMap`).
    let mut cache: HashMap<String, CachedConn> = HashMap::new();
    // True once we've already invoked `handle_terminal_event` +
    // `forward_disconnect_to_broker` for this connection. Terminal `Error`
    // and `Disconnected` ARE both expected on the genuine teardown path
    // (`connection.rs:493` → `run_connection` unwind → `Disconnected`), and
    // either one alone is also valid: a `Disconnected` without preceding
    // Error fires for clean transport close, and a terminal Error in
    // theory could be the last event if the bus drops the trailing
    // Disconnected (broadcast `Lagged`). Whichever lands first dispatches
    // the terminal work; the second one is a no-op so the broker / DB
    // aren't double-touched.
    let mut terminal_dispatched = false;
    while let Some(envelope_arc) = rx.recv().await {
        let envelope: &EventEnvelope = envelope_arc.as_ref();
        match &envelope.payload {
            AcpEvent::ConversationLinked {
                conversation_id, ..
            } => {
                try_cache_link(&mut cache, &manager, &connection_id, *conversation_id).await;
                if let Err(error) = crate::acp::agent_input_lifecycle::recover_connection(
                    &db,
                    &manager,
                    &connection_id,
                    *conversation_id,
                )
                .await
                {
                    tracing::error!(
                        connection_id = %connection_id,
                        conversation_id,
                        error = %error,
                        "[agent-input] connection recovery failed"
                    );
                }
                handle_event_with_retry(
                    &db,
                    &manager,
                    &chat_channel_manager,
                    envelope,
                    broker.as_ref(),
                    harvest_service.as_ref(),
                )
                .await;
            }
            AcpEvent::TurnComplete { .. } => {
                finish_browser_turn(browser.as_ref(), &manager, &envelope.connection_id).await;
                handle_event_with_retry(
                    &db,
                    &manager,
                    &chat_channel_manager,
                    envelope,
                    broker.as_ref(),
                    harvest_service.as_ref(),
                )
                .await;
            }
            AcpEvent::ConversationStatusChanged {
                status: ConversationStatus::Cancelled,
                ..
            } => {
                finish_browser_turn(browser.as_ref(), &manager, &envelope.connection_id).await;
            }
            AcpEvent::StatusChanged {
                status: ConnectionStatus::Disconnected,
            } => {
                if terminal_dispatched {
                    continue;
                }
                if let Err(e) = handle_terminal_event(&db, &mut cache, &connection_id).await {
                    tracing::error!("[lifecycle][ERROR] terminal event for {connection_id}: {e}");
                }
                if let Some(b) = broker.as_ref() {
                    forward_disconnect_to_broker(b.as_ref(), &connection_id, None).await;
                }
                finish_browser_connection(browser.as_ref(), &connection_id).await;
                terminal_dispatched = true;
            }
            AcpEvent::Error {
                message,
                code,
                terminal,
                ..
            } => {
                // Non-terminal Errors (`turn_failure_error_event`,
                // `session/load` fallback, empty-prompt rejection, SetMode
                // / SetConfigOption failures) leave the connection alive:
                // - flipping the row InProgress → Cancelled would briefly
                //   show "Cancelled" in the UI before the next TurnComplete
                //   corrects it (cosmetic but jumpy).
                // - draining the broker would race-cancel a pending
                //   delegation that the upcoming `TurnComplete` →
                //   `complete_call` would have mapped to a proper child-side
                //   error code (`ChildRefusal` / `ChildMaxTokens` / …).
                //
                // F2 in the v0.14.3 sub-agent delegation post-mortem.
                if !*terminal {
                    continue;
                }
                if terminal_dispatched {
                    continue;
                }
                // Genuinely terminal (the `run_connection` failure path at
                // `connection.rs:493`). Drain the broker NOW with the error
                // detail instead of waiting for the trailing `Disconnected`.
                // If `Disconnected` never arrives (bus `Lagged`, task
                // abort, a future emit site that forgets to follow up) the
                // parent's `delegate_to_agent` would otherwise block on
                // `rx.await` forever. The drain itself is idempotent
                // (`cancel_by_child_connection` no-ops on empty pending),
                // so the subsequent Disconnected will short-circuit on
                // `terminal_dispatched`.
                if let Err(e) = handle_terminal_event(&db, &mut cache, &connection_id).await {
                    tracing::error!("[lifecycle][ERROR] terminal event for {connection_id}: {e}");
                }
                if let Some(b) = broker.as_ref() {
                    let detail = format_terminal_error(message, code.as_deref());
                    forward_disconnect_to_broker(b.as_ref(), &connection_id, Some(&detail)).await;
                }
                finish_browser_connection(browser.as_ref(), &connection_id).await;
                terminal_dispatched = true;
            }
            _ => {
                handle_event_with_retry(
                    &db,
                    &manager,
                    &chat_channel_manager,
                    envelope,
                    broker.as_ref(),
                    harvest_service.as_ref(),
                )
                .await;
            }
        }
    }
}

async fn finish_browser_turn(
    browser: Option<&BrowserSessionManager>,
    manager: &ConnectionManager,
    connection_id: &str,
) {
    let Some(browser) = browser else { return };
    let Some(state) = manager.get_state(connection_id).await else {
        browser.finish_agent_connection(connection_id).await;
        return;
    };
    let turn_generation = state.read().await.turn_generation;
    browser
        .finish_agent_turn(connection_id, turn_generation)
        .await;
}

async fn finish_browser_connection(browser: Option<&BrowserSessionManager>, connection_id: &str) {
    if let Some(browser) = browser {
        browser.finish_agent_connection(connection_id).await;
    }
}

/// Subscribe to the in-process bus synchronously and return the dispatcher
/// loop future. Filters out events the lifecycle worker doesn't care about
/// (high-frequency ContentDelta / ToolCall / PermissionRequest etc.) and
/// fans the rest out to per-connection worker tasks. Within a single
/// connection, ordering is preserved by the per-worker mpsc; across
/// connections, workers run independently so a slow SQLite write on one
/// connection doesn't backpressure the others.
///
/// All forwarded events selected by `is_lifecycle_relevant` use
/// blocking `send().await` to guarantee delivery even when the worker
/// mailbox is full — `SessionStarted` (writes external_id) and
/// `TurnComplete` (writes terminal status) are correctness-critical and
/// silently dropping either leaves the conversation row in a permanently
/// wrong state. Filtering keeps the queue from filling on noise traffic
/// so the blocking path is rarely exercised.
///
/// The `subscribe()` call happens here, before the future is returned, so any
/// events emitted between this call and the first poll are buffered by the
/// broadcast channel rather than dropped.
pub fn lifecycle_subscriber_task(
    db_conn: DatabaseConnection,
    manager: ConnectionManager,
    chat_channel_manager: ChatChannelManager,
    bus: Arc<InternalEventBus>,
    broker: Option<Arc<DelegationBroker>>,
    harvest_service: Option<Arc<UserMemoryService>>,
    browser: Option<BrowserSessionManager>,
) -> impl Future<Output = ()> + Send + 'static {
    let mut rx = bus.subscribe();
    let metrics = Arc::clone(bus.metrics());
    async move {
        // connection_id → worker mailbox. Workers are spawned lazily on the
        // connection's first relevant event and torn down after a terminal
        // event by dropping the sender (worker drains its queue and exits).
        let mut workers: HashMap<String, mpsc::Sender<Arc<EventEnvelope>>> = HashMap::new();
        loop {
            match rx.recv().await {
                Ok(envelope_arc) => {
                    // Off-worker delegation correlation. Register parent-side
                    // `delegate_to_agent` tool_call_ids the instant they come
                    // off the bus — before the `is_lifecycle_relevant` filter
                    // and before any worker `send().await` that could block and
                    // back-pressure the bus into dropping a later event. This is
                    // why `ToolCall`/`ToolCallUpdate` no longer need to reach a
                    // worker at all. See `register_delegation_tool_call_from_event`.
                    if let Some(b) = broker.as_ref() {
                        register_delegation_tool_call_from_event(b.as_ref(), &envelope_arc).await;
                    }

                    // Fast-path filter: skip events the worker would no-op.
                    // Avoids spawning a worker for connections that only emit
                    // high-frequency noise and avoids crowding existing
                    // workers' mailboxes.
                    if !is_lifecycle_relevant(&envelope_arc.payload) {
                        continue;
                    }

                    let conn_id = envelope_arc.connection_id.clone();
                    let is_terminal = is_dispatcher_terminal(&envelope_arc.payload);

                    let tx = workers.entry(conn_id.clone()).or_insert_with(|| {
                        let (tx, worker_rx) =
                            mpsc::channel::<Arc<EventEnvelope>>(WORKER_QUEUE_CAPACITY);
                        let db_clone = db_conn.clone();
                        let mgr_clone = manager.clone_ref();
                        let chat_channel_clone = chat_channel_manager.clone_ref();
                        let broker_clone = broker.clone();
                        let harvest_clone = harvest_service.clone();
                        let browser_clone = browser.clone();
                        let id_clone = conn_id.clone();
                        tokio::spawn(connection_worker_loop(
                            id_clone,
                            db_clone,
                            mgr_clone,
                            chat_channel_clone,
                            broker_clone,
                            harvest_clone,
                            browser_clone,
                            worker_rx,
                        ));
                        tx
                    });

                    // Two-phase send: try non-blocking first (the common
                    // case), only `await` when the mailbox is actually full.
                    // Counts queue-full as back-pressure observation rather
                    // than a drop event — nothing is dropped, the dispatcher
                    // just waits for the worker to make room.
                    let send_result = match tx.try_send(envelope_arc) {
                        Ok(()) => Ok(()),
                        Err(mpsc::error::TrySendError::Full(env)) => {
                            metrics
                                .worker_queue_full_count
                                .fetch_add(1, Ordering::Relaxed);
                            tracing::warn!(
                                "[lifecycle][WARN] worker queue full for \
                                 {conn_id}, awaiting drain (back-pressure)"
                            );
                            tx.send(env).await.map_err(|_| ())
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => Err(()),
                    };

                    if send_result.is_err() {
                        // Worker exited unexpectedly (panic). Clean up the
                        // stale entry so the next relevant event respawns.
                        workers.remove(&conn_id);
                        continue;
                    }

                    if is_terminal {
                        // Drop the sender; worker drains the queue then
                        // exits. Releases the per-connection `CachedConn`
                        // (state Arc + emitter) the worker was holding.
                        workers.remove(&conn_id);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    // Lagged at the bus level. Now that the dispatcher
                    // filters and only blocks on the rare relevant events,
                    // this should only fire under genuine emit-rate spikes
                    // exceeding the 4096 broadcast capacity.
                    tracing::warn!(
                        "[lifecycle][WARN] internal bus lagged, dropped {skipped} events \
                         (emit rate exceeded broadcast capacity)"
                    );
                    metrics.lagged_count.fetch_add(skipped, Ordering::Relaxed);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("[lifecycle] internal bus closed; dispatcher exiting");
                    // Drop all worker senders; workers drain & exit on their own.
                    drop(workers);
                    break;
                }
            }
        }
    }
}
