//! Background task that aggregates broadcaster events into two pet streams:
//!
//! * `pet://state` — the *ambient* `PetState` derived from cross-connection
//!   ACP signals (idle/waiting/running/failed). De-duplicated; only emitted
//!   when the computed state changes.
//! * `pet://oneshot` — *transient* feedback animations triggered by discrete
//!   events (PendingReview, failed turn_complete stop reasons, git
//!   commit/push, merge abort, agent install, manual `pet_celebrate` calls).
//!   Always emitted; the renderer plays a few loops and falls back to the
//!   current ambient state.
//!
//! Phase 5: ACP events are now consumed from `InternalEventBus`
//! (`Arc<EventEnvelope>`, no JSON parse), while folder/app non-ACP channels
//! continue to flow through `WebEventBroadcaster`. The subscriber selects
//! over both receivers in the same `tokio::select!` loop.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::acp::internal_bus::InternalEventBus;
use crate::acp::types::{AcpEvent, ConnectionStatus, EventEnvelope};
use crate::db::entities::conversation::ConversationStatus;
use crate::models::pet::PetState;
use crate::web::event_bridge::{emit_event, EventEmitter, WebEvent, WebEventBroadcaster};

/// Shared latest-known `PetState`, written by the subscriber task and read
/// by the `pet_get_current_state` command. Lets a freshly opened pet window
/// pick up the *current* ambient state instead of waiting for the next
/// state transition — the subscriber only emits on changes, so without this
/// the frontend would otherwise sit on its default `Idle` indefinitely if
/// the agent was already running when the window opened.
pub type PetStateHandle = Arc<RwLock<PetState>>;

pub fn new_pet_state_handle() -> PetStateHandle {
    Arc::new(RwLock::new(PetState::Idle))
}

/// Read the current ambient state. Falls back to `Idle` if the lock is
/// poisoned — a poisoned lock means the writer panicked, in which case
/// the snapshot is stale and `Idle` is the safe default.
pub fn read_pet_state(handle: &PetStateHandle) -> PetState {
    handle.read().map(|guard| *guard).unwrap_or(PetState::Idle)
}

fn write_pet_state(handle: &PetStateHandle, value: PetState) {
    match handle.write() {
        Ok(mut guard) => *guard = value,
        Err(err) => {
            // A poisoned lock means a previous writer panicked. The handle
            // is now permanently stale, which would silently degrade the
            // open-pet-mid-conversation experience to "always Idle" with no
            // other symptom. Surface it so it shows up in operator logs.
            tracing::info!("[Pet] pet_state handle poisoned, dropping write: {err}");
        }
    }
}

/// How long the ambient `Failed` state stays visible before automatically
/// fading back to whatever the rest of the snapshot would compute. Restarts
/// each time a fresh error event arrives.
const PET_FAILED_RECOVERY_MS: u64 = 4_000;

/// Aggregate snapshot of cross-connection ACP signals, derived from the
/// stream of `AcpEvent`s. Pure data — `compute_pet_state` is the sole
/// source of truth for translating it into a `PetState`.
#[derive(Debug, Clone, Default)]
pub struct PetGlobalState {
    /// Connections currently in `Prompting` (an in-flight prompt is streaming).
    prompting: HashSet<String>,
    /// Connections in a terminal `Error` state. We treat any error event as
    /// authoritative even if a later `StatusChanged` clears it — Codex's
    /// `failed` row should briefly play, then the next event will reset it.
    erroring: HashSet<String>,
    /// Outstanding permission requests (request_id → connection_id). The
    /// presence of *any* outstanding permission keeps ambient state at
    /// `Waiting` until the user resolves it.
    pending_permissions: HashMap<String, String>,
    /// Connection ids of in-flight delegation sub-agents. Their status events
    /// ride the parent's emitter onto this bus, but a sub-agent is not a
    /// user-facing session — it's surfaced inline in its parent's transcript
    /// and excluded from the pet badge/panel. We track child ids here and
    /// ignore their ambient signals so the pet's busy state stays consistent
    /// with the session list (otherwise the pet looks "running" while the
    /// panel shows nothing). Populated by `DelegationStarted`, cleared by
    /// `DelegationCompleted`.
    delegation_children: HashSet<String>,
}

impl PetGlobalState {
    pub fn apply(&mut self, env: &EventEnvelope) {
        let conn = &env.connection_id;
        match &env.payload {
            // A delegation sub-agent is not a user-facing session. Remember its
            // connection id so its own ambient signals are ignored, and scrub
            // anything those signals may have recorded already — the child's
            // `StatusChanged{Prompting}` can reach the bus just before this
            // event does. (These two events carry the *parent's* connection id
            // in the envelope; the child id is in the payload.)
            AcpEvent::DelegationStarted {
                child_connection_id,
                ..
            } => {
                self.delegation_children.insert(child_connection_id.clone());
                self.prompting.remove(child_connection_id);
                self.erroring.remove(child_connection_id);
                self.pending_permissions
                    .retain(|_, cid| cid != child_connection_id);
            }
            AcpEvent::DelegationCompleted {
                child_connection_id,
                ..
            } => {
                self.delegation_children.remove(child_connection_id);
                self.prompting.remove(child_connection_id);
                self.erroring.remove(child_connection_id);
                self.pending_permissions
                    .retain(|_, cid| cid != child_connection_id);
            }
            // A known sub-agent's signals never drive ambient state. When it
            // disconnects, forget it so dead ids don't pile up — this also
            // cleans up a `DelegationCompleted` that was dropped on a bus
            // overrun (the child id is otherwise preserved across the reset).
            _ if self.delegation_children.contains(conn) => {
                if matches!(
                    &env.payload,
                    AcpEvent::StatusChanged {
                        status: ConnectionStatus::Disconnected,
                    }
                ) {
                    self.delegation_children.remove(conn);
                }
            }
            AcpEvent::StatusChanged { status } => match status {
                ConnectionStatus::Prompting => {
                    self.prompting.insert(conn.clone());
                    self.erroring.remove(conn);
                }
                ConnectionStatus::Connected | ConnectionStatus::Connecting => {
                    self.prompting.remove(conn);
                    self.erroring.remove(conn);
                }
                ConnectionStatus::Error => {
                    self.erroring.insert(conn.clone());
                    self.prompting.remove(conn);
                }
                ConnectionStatus::Disconnected => {
                    self.prompting.remove(conn);
                    self.erroring.remove(conn);
                    self.pending_permissions.retain(|_, cid| cid != conn);
                }
            },
            AcpEvent::Error { .. } => {
                self.erroring.insert(conn.clone());
            }
            AcpEvent::PermissionRequest { request_id, .. } => {
                self.pending_permissions
                    .insert(request_id.clone(), conn.clone());
            }
            AcpEvent::PermissionResolved { request_id } => {
                // User answered (allow/reject) or chat-channel auto-approve
                // ran. responder.respond() is RPC-only with no follow-up
                // event of its own, so the connection emits this synthetic
                // event right after sending the response. Without it the
                // entry lives on until TurnComplete, which for Plan
                // approvals (ExitPlanMode) is the entire post-approval
                // implementation window — pinning the pet on Waiting
                // throughout.
                self.pending_permissions.remove(request_id);
            }
            AcpEvent::TurnComplete { .. } => {
                self.prompting.remove(conn);
                // A permission request is bounded by the turn that raised it:
                // by the time TurnComplete arrives the user has either
                // approved (agent reached end_turn / refusal / max_tokens)
                // or the turn was cancelled. There is no separate event
                // when the user clicks allow/deny — the response goes
                // straight back to the agent through `responder.respond()`
                // — so this is the only deterministic place to drop the
                // entry. Without this, a single past permission would mask
                // Running across the entire app until the connection drops.
                self.pending_permissions.retain(|_, cid| cid != conn);
            }
            _ => {}
        }
    }

    /// Reset the volatile signal sets after a bus overrun. We can't reconstruct
    /// `prompting` / `erroring` / `pending_permissions` from dropped events, so
    /// we clear them and let the next `StatusChanged` batch reseed. Crucially
    /// `delegation_children` is PRESERVED: an in-flight sub-agent won't
    /// re-announce itself with another `DelegationStarted`, so dropping its
    /// classification here would let its later events be mistaken for a real
    /// session — the pet would look "running" while the badge/panel still
    /// exclude it. A child whose `DelegationCompleted` was also dropped is
    /// cleaned up when its connection disconnects (and is otherwise harmless —
    /// connection ids are never reused).
    fn reset_after_overrun(&mut self) {
        self.prompting.clear();
        self.erroring.clear();
        self.pending_permissions.clear();
    }
}

/// Pure function: aggregate → state. Order of checks defines priority.
///
/// Priority rationale, top-down:
///
/// * `Failed` — most urgent, brief auto-recovery handles the linger.
/// * `Waiting` from `pending_permissions` — blocking: the agent literally
///   cannot proceed without the user clicking allow/deny, so it outranks
///   any concurrent prompt elsewhere. Renders as `Waiting` (not a separate
///   highlight) so the cue blends with the regular idle-but-reachable
///   state; the actual permission dialog is what demands the user's
///   attention, the pet just stops looking busy.
/// * `Running` from `prompting` — active work elsewhere.
/// * `Idle` — nothing blocking or running.
///
/// `ConversationStatus::PendingReview` no longer feeds ambient state: it
/// fires a one-shot `pet://oneshot = "review"` cue at the moment the
/// review becomes pending, then the pet returns to whichever ambient
/// state the snapshot computes. See the subscriber loop for the trigger.
pub fn compute_pet_state(snapshot: &PetGlobalState) -> PetState {
    if !snapshot.erroring.is_empty() {
        return PetState::Failed;
    }
    if !snapshot.pending_permissions.is_empty() {
        return PetState::Waiting;
    }
    if !snapshot.prompting.is_empty() {
        return PetState::Running;
    }
    PetState::Idle
}

/// Pet only reacts to a small subset of ACP event types. Filtering at the
/// dispatcher level avoids cloning / matching downstream when the bus
/// fires for high-volume content / tool / thinking deltas.
fn is_acp_event_relevant(payload: &AcpEvent) -> bool {
    matches!(
        payload,
        AcpEvent::StatusChanged { .. }
            | AcpEvent::Error { .. }
            | AcpEvent::PermissionRequest { .. }
            | AcpEvent::PermissionResolved { .. }
            | AcpEvent::TurnComplete { .. }
            | AcpEvent::ConversationStatusChanged { .. }
            // Tracked so a sub-agent's connection can be filtered out of
            // ambient state (it must not make the pet look "running" when the
            // panel — which excludes sub-agents — shows nothing).
            | AcpEvent::DelegationStarted { .. }
            | AcpEvent::DelegationCompleted { .. }
    )
}

/// Map a `TurnComplete.stop_reason` to a oneshot animation, if any. Successful
/// turns are represented by the subsequent `PendingReview` transition so the
/// renderer receives exactly one completion cue.
fn classify_turn_complete(stop_reason: &str) -> Option<PetState> {
    match stop_reason {
        "refusal" | "max_tokens" | "max_turn_requests" | "unknown" | "empty" => {
            Some(PetState::Failed)
        }
        // `end_turn` is covered by PendingReview; `cancelled` and future reasons stay silent.
        _ => None,
    }
}

/// Map an `app://agent-install` event payload to a oneshot animation.
/// `started` / `log` are noisy progress signals; only the terminal kinds
/// `completed` / `failed` produce a reaction.
fn classify_agent_install(payload: &serde_json::Value) -> Option<PetState> {
    let kind = payload.get("kind").and_then(|v| v.as_str())?;
    match kind {
        "completed" => Some(PetState::Jumping),
        "failed" => Some(PetState::Failed),
        _ => None,
    }
}

fn emit_oneshot(emitter: &EventEmitter, kind: PetState) {
    emit_event(emitter, "pet://oneshot", kind);
}

/// Schedule (or restart) the auto-recovery timer that will clear the
/// `erroring` set after `PET_FAILED_RECOVERY_MS`. Aborts any in-flight
/// timer first so successive errors keep the failed animation visible
/// for the full window after the *latest* error.
fn schedule_failed_recovery(clear_task: &mut Option<JoinHandle<()>>, clear_tx: &mpsc::Sender<()>) {
    cancel_failed_recovery(clear_task);
    let tx = clear_tx.clone();
    *clear_task = Some(tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(PET_FAILED_RECOVERY_MS)).await;
        // `try_send` instead of awaiting: the channel is sized for the
        // worst case (8 messages) and the main loop is the only consumer,
        // so the only way send would block is a stuck consumer — in which
        // case adding more messages can't help. A drop here just means
        // the failed animation lingers slightly longer than the window,
        // which is benign.
        let _ = tx.try_send(());
    }));
}

fn cancel_failed_recovery(clear_task: &mut Option<JoinHandle<()>>) {
    if let Some(t) = clear_task.take() {
        t.abort();
    }
}

/// Spawn-friendly subscriber loop. Mirrors `lifecycle_subscriber_task`'s
/// "subscribe synchronously, return future" shape so each broadcast/bus
/// buffer covers the gap between `subscribe()` and the first `recv()`.
///
/// Dual-source design: ACP envelopes (typed) flow in through `bus`, while
/// folder/app side-channel notifications stay on `broadcaster`. The two
/// receivers are select!'d over a shared `select!` arm; every other arm
/// (recovery timer, channel close) is unchanged from the single-source
/// version. Splitting buses lets ACP consumers skip the JSON reparse and
/// — crucially — lets us drop the `acp://event` channel from the global
/// firehose entirely (eliminating the WS receiver-side dedup hack).
pub fn pet_state_subscriber_task(
    bus: Arc<InternalEventBus>,
    broadcaster: Arc<WebEventBroadcaster>,
    emitter: EventEmitter,
    handle: PetStateHandle,
) -> impl Future<Output = ()> + Send + 'static {
    let mut acp_rx = bus.subscribe();
    let mut web_rx = broadcaster.subscribe();
    let metrics = Arc::clone(bus.metrics());
    let (clear_tx, mut clear_rx) = mpsc::channel::<()>(8);
    async move {
        let mut snapshot = PetGlobalState::default();
        let mut last_state = PetState::Idle;
        let mut clear_task: Option<JoinHandle<()>> = None;
        // Push an initial "idle" snapshot so the renderer doesn't start blank.
        write_pet_state(&handle, last_state);
        emit_event(&emitter, "pet://state", last_state);

        loop {
            tokio::select! {
                acp_event = acp_rx.recv() => {
                    match acp_event {
                        Ok(envelope_arc) => {
                            let envelope = envelope_arc.as_ref();
                            if !is_acp_event_relevant(&envelope.payload) {
                                continue;
                            }

                            // Fire the turn_complete oneshot *before*
                            // applying — the apply step removes the
                            // connection from `prompting`, but the
                            // celebration should reference the turn
                            // that just ended either way.
                            if let AcpEvent::TurnComplete { stop_reason, .. } =
                                &envelope.payload
                            {
                                if let Some(kind) = classify_turn_complete(stop_reason) {
                                    emit_oneshot(&emitter, kind);
                                }
                            }

                            // PendingReview fires a one-shot cue rather than
                            // ambient state, so a single un-acked review can't
                            // pin the pet on `review` for the rest of the
                            // session.
                            if let AcpEvent::ConversationStatusChanged {
                                status: ConversationStatus::PendingReview,
                                ..
                            } = &envelope.payload
                            {
                                emit_oneshot(&emitter, PetState::Review);
                            }

                            let was_erroring = !snapshot.erroring.is_empty();
                            snapshot.apply(envelope);
                            let now_erroring = !snapshot.erroring.is_empty();

                            let triggered_error = matches!(
                                envelope.payload,
                                AcpEvent::Error { .. }
                                    | AcpEvent::StatusChanged {
                                        status: ConnectionStatus::Error,
                                    }
                            );
                            if triggered_error && now_erroring {
                                schedule_failed_recovery(&mut clear_task, &clear_tx);
                            } else if was_erroring && !now_erroring {
                                // erroring went empty without us firing the
                                // recovery timer — e.g. Connected/Disconnected
                                // events that pruned the last erroring conn —
                                // so cancel the pending sleep to avoid a
                                // phantom recompute later.
                                cancel_failed_recovery(&mut clear_task);
                            }
                            // ACP is the only path that mutates `snapshot`,
                            // so ambient recompute is gated to this arm.
                            let next = compute_pet_state(&snapshot);
                            if next != last_state {
                                last_state = next;
                                write_pet_state(&handle, next);
                                emit_event(&emitter, "pet://state", next);
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            // Bus overrun — we can't reliably reconstruct
                            // state from the missed events, so reset to Idle
                            // and rely on the next batch of
                            // StatusChanged/Connected events to reseed the
                            // snapshot. A persistent lag without follow-up
                            // events would leave the pet stuck on idle even
                            // if connections are still active; surface it on
                            // the metric so operators can spot it.
                            tracing::warn!(
                                "[Pet] internal bus lagged, dropped {skipped} events; resetting to idle"
                            );
                            metrics.lagged_count.fetch_add(skipped, Ordering::Relaxed);
                            // Clear the volatile signals (reseeded from the next
                            // StatusChanged batch) but KEEP the delegation-child
                            // classification — a running sub-agent won't re-fire
                            // DelegationStarted, so forgetting it here would let
                            // its events drive ambient state again (the bug this
                            // exclusion fixes), under the overrun recovery path.
                            snapshot.reset_after_overrun();
                            cancel_failed_recovery(&mut clear_task);
                            if last_state != PetState::Idle {
                                last_state = PetState::Idle;
                                write_pet_state(&handle, last_state);
                                emit_event(&emitter, "pet://state", last_state);
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            cancel_failed_recovery(&mut clear_task);
                            break;
                        }
                    }
                }
                web_event = web_rx.recv() => {
                    match web_event {
                        Ok(WebEvent { channel, payload }) => {
                            // Folder/app side-channels stay on the JSON
                            // broadcaster: they aren't ACP envelopes, and the
                            // emitters (folder commands, agent installer)
                            // don't go through `emit_with_state`.
                            match channel.as_str() {
                                "folder://git-commit-succeeded"
                                | "folder://git-push-succeeded" => {
                                    emit_oneshot(&emitter, PetState::Jumping);
                                }
                                "folder://merge-aborted" => {
                                    emit_oneshot(&emitter, PetState::Failed);
                                }
                                "app://agent-install" => {
                                    if let Some(kind) = classify_agent_install(payload.as_ref()) {
                                        emit_oneshot(&emitter, kind);
                                    }
                                }
                                _ => continue,
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            // Web broadcaster lag is a different signal:
                            // these are fire-and-forget side-channel events
                            // (a lost git-commit ping is benign), so just
                            // log and keep going. No snapshot reset.
                            tracing::warn!(
                                "[Pet] web broadcaster lagged, dropped {skipped} non-ACP events"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            cancel_failed_recovery(&mut clear_task);
                            break;
                        }
                    }
                }
                Some(_) = clear_rx.recv() => {
                    // Recovery timer fired — drop the failed-state lock and
                    // recompute the ambient state from whatever else is
                    // currently active.
                    snapshot.erroring.clear();
                    clear_task = None;
                    let next = compute_pet_state(&snapshot);
                    if next != last_state {
                        last_state = next;
                        write_pet_state(&handle, next);
                        emit_event(&emitter, "pet://state", next);
                    }
                }
            }
        }
    }
}
