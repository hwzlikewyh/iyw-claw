//! `DelegationBroker` — the coordination unit for multi-agent delegation.
//!
//! Delegation is **asynchronous**: `delegate_to_agent` returns a `task_id`
//! ack as soon as setup finishes; the LLM collects the result later with
//! `get_delegation_status` (optionally long-polling) or stops it with
//! `cancel_delegation`. There is no blocking `oneshot` — a running task is just
//! an entry in the `running` map, and a terminal event migrates it into the
//! `completed` cache (atomically, under one lock) and wakes any long-poll via
//! `result_notify`.
//!
//! Lifecycle of a single task:
//!
//! 1. [`DelegationBroker::start_delegation`] is the broker's entry point. The
//!    MCP listener feeds it the LLM-issued `delegate_to_agent` payload.
//! 2. Pre-checks: feature enabled? depth limit ok? Both failures return a
//!    terminal report immediately, no child session created.
//! 3. Spawn the child via [`ConnectionSpawner::spawn`].
//! 4. Send the delegation task as the first prompt via
//!    [`ConnectionSpawner::send_prompt_linked_for_delegation`]. The trailing
//!    [`DelegationLink`] carries the parent's `tool_use_id` and a
//!    broker-internal `call_id` (UUID = `task_id`) — persisted onto the new
//!    conversation row so the lifecycle resolver can find it.
//! 5. Register a [`RunningTask`] keyed by `call_id` and return a `Running` ack
//!    [`DelegationTaskReport`] (or a terminal report when the child finished
//!    during setup / a cancel reached it mid-setup / setup itself failed).
//! 6. Later, a terminal event resolves the task — migrating it `running` →
//!    `completed` and tearing the child down:
//!       - the lifecycle calling [`DelegationBroker::complete_call`] on
//!         `TurnComplete` (happy path), or
//!       - a cancel — MCP-side (`notifications/cancelled` →
//!         [`DelegationBroker::cancel_by_external_handle`]), child-side
//!         ([`DelegationBroker::cancel_by_child_connection`]), parent-side
//!         ([`DelegationBroker::cancel_by_parent`] /
//!         [`DelegationBroker::cancel_by_parent_turn`]), or the LLM's own
//!         [`DelegationBroker::cancel_task_by_id`].
//!
//! v1 is explicitly one-shot — no session reuse.
//!
//! Result durability: child output is NOT stored in iyw-claw's DB, so the broker
//! caches the completed text in `completed` (parent-scoped, FIFO-capped). Once
//! evicted, [`DelegationBroker::get_task_status`] falls back to the DB for the
//! task's terminal STATUS (via [`ChildStatusLookup`]); the full output is always
//! viewable in the child's own session.
//!
//! Cancellation cascade: when a parent session goes away (user-initiated
//! cancel, parent disconnect), the lifecycle subscriber calls
//! [`DelegationBroker::cancel_by_parent`] which fans out cancel + disconnect
//! to every running child of that parent. A normal `end_turn` does NOT cancel
//! children — they keep running in the background (the whole point of async).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore};

use crate::acp::automatic_mode::automatic_mode_id;
use crate::acp::delegation::event_emitter::{DelegationEventEmitter, NoopEventEmitter};
use crate::acp::delegation::live_reply::{ChildLiveReplyLookup, NoopChildLiveReplyLookup};
use crate::acp::delegation::meta_writer::{
    build_delegation_meta, is_synthetic_parent_tool_use_id, DelegationMetaWriter, NoopMetaWriter,
};
use crate::acp::delegation::spawner::{ConnectionSpawner, DelegationLink};
use crate::acp::delegation::types::{
    AgentDelegationDefaults, DelegationError, DelegationOutcome, DelegationRequest,
    DelegationTaskReport, TaskStatus,
};
use crate::acp::types::DelegationResultSummary;
use crate::models::AgentType;

pub(crate) const DELEGATION_ACK_PREFIX: &str = "Delegation successful. task_id=";
pub(crate) const DELEGATION_ACK_SUFFIX: &str =
    ". Call get_delegation_status with this id in the task_ids \
array (optionally wait_ms) to collect the result, or cancel_delegation to stop it.";

fn parse_delegation_ack_text(text: &str) -> Result<Option<String>, ()> {
    let text = text.trim();
    let has_marker = text.contains(DELEGATION_ACK_PREFIX) || text.contains("task_id=");
    let Some(task_id) = text
        .strip_prefix(DELEGATION_ACK_PREFIX)
        .and_then(|rest| rest.strip_suffix(DELEGATION_ACK_SUFFIX))
    else {
        return if has_marker { Err(()) } else { Ok(None) };
    };
    let parsed = uuid::Uuid::parse_str(task_id).map_err(|_| ())?;
    Ok(Some(parsed.to_string()))
}

fn validate_structured_task_id(
    object: &serde_json::Map<String, serde_json::Value>,
    task_id: &str,
) -> Result<(), ()> {
    let Some(value) = object
        .get("structuredContent")
        .and_then(|value| value.get("task_id"))
    else {
        return Ok(());
    };
    let parsed = uuid::Uuid::parse_str(value.as_str().ok_or(())?).map_err(|_| ())?;
    (parsed.to_string() == task_id).then_some(()).ok_or(())
}

fn parse_structured_delegation_ack(value: &serde_json::Value) -> Result<Option<String>, ()> {
    let Some(object) = value.as_object() else {
        return Ok(None);
    };
    let Some([block]) = object
        .get("content")
        .and_then(|value| value.as_array())
        .map(Vec::as_slice)
    else {
        return if value.to_string().contains("task_id=") {
            Err(())
        } else {
            Ok(None)
        };
    };
    if block.get("type").and_then(|value| value.as_str()) != Some("text") {
        return Ok(None);
    }
    let Some(task_id) = parse_delegation_ack_text(
        block
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or_default(),
    )?
    else {
        return Ok(None);
    };
    if object.get("isError").and_then(|value| value.as_bool()) != Some(false) {
        return Err(());
    }
    validate_structured_task_id(object, &task_id)?;
    Ok(Some(task_id))
}

fn parse_delegation_ack_payload(payload: &str) -> Result<Option<String>, ()> {
    let trimmed = payload.trim();
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => parse_structured_delegation_ack(&value),
        Err(_) => parse_delegation_ack_text(trimmed),
    }
}

pub(crate) fn delegation_ack_task_id(
    content: Option<&str>,
    raw_output: Option<&str>,
) -> Option<String> {
    let mut found: Option<String> = None;
    for payload in [content, raw_output].into_iter().flatten() {
        let Some(task_id) = parse_delegation_ack_payload(payload).ok()? else {
            continue;
        };
        if found.as_ref().is_some_and(|existing| existing != &task_id) {
            return None;
        }
        found = Some(task_id);
    }
    found
}

/// Default per-parent byte budget for cached completed-task result text. The
/// completed-cache lets `get_delegation_status` / `cancel_delegation` return a
/// finished task's result after the lifecycle resolved it; once a parent's
/// retained result text exceeds this budget the OLDEST results are FIFO-evicted
/// (evicted tasks fall back to the DB status lookup, which carries status only).
/// This is the seed value baked into `DelegationConfig::default()`; the live
/// value is user-configurable from the settings page (in MB) and `0` disables
/// eviction entirely. See `PendingInner::completed_cap_bytes`.
const DEFAULT_COMPLETED_CACHE_CAP_BYTES: usize = 512 * 1024 * 1024;

/// Per-result cap on cached completed text. The full child output always lives
/// in the child's own session (viewable via the frontend's child-session
/// sheet); this only bounds the broker's in-memory copy of a SINGLE result.
/// Because it is far below the per-parent byte budget
/// (`DEFAULT_COMPLETED_CACHE_CAP_BYTES`), the newest result always fits and is
/// never the eviction victim in `insert_completed`.
const COMPLETED_TEXT_CAP: usize = 256 * 1024;

/// Cap on the inline `text_preview` carried by the `DelegationCompleted` event
/// and the terminal meta, so the parent card can render the result inline
/// without re-fetching the child session.
const STATUS_PREVIEW_CAP: usize = 2 * 1024;

/// Lookup the `parent_id` for a conversation. Abstracted so the broker can be
/// unit-tested against an in-memory chain without touching SeaORM.
#[async_trait]
pub trait ConversationDepthLookup: Send + Sync {
    async fn parent_of(&self, conversation_id: i32) -> Result<Option<i32>, DelegationError>;
}

/// Status-level facts the broker recovers from a child conversation row when a
/// task's in-memory completed-cache entry was evicted. Carries NO result text —
/// child output isn't stored in iyw-claw's DB; the full result lives in the
/// child's own session (viewable via the frontend's child-session sheet).
#[derive(Debug, Clone)]
pub struct ChildStatusRecord {
    pub child_conversation_id: i32,
    pub status: TaskStatus,
    pub agent_type: AgentType,
    /// The parent conversation id this child was spawned under. Used to scope
    /// the DB fallback to the calling parent so one parent can't read another's
    /// task by guessing a UUID.
    pub parent_id: Option<i32>,
}

/// DB fallback for `get_delegation_status` / `cancel_delegation` once a task's
/// result has aged out of the broker's in-memory completed-cache. Abstracted
/// so broker unit tests can run without SeaORM; production wires
/// [`DbChildStatusLookup`] via [`DelegationBroker::with_status_lookup`].
#[async_trait]
pub trait ChildStatusLookup: Send + Sync {
    async fn find_by_call_id(&self, call_id: &str) -> Option<ChildStatusRecord>;
}

/// Default lookup — always "unknown". Used by `DelegationBroker::new` /
/// `with_writers` (tests that don't exercise the DB-fallback path); production
/// replaces it via `with_status_lookup`.
#[derive(Default, Clone)]
pub struct NoopChildStatusLookup;

#[async_trait]
impl ChildStatusLookup for NoopChildStatusLookup {
    async fn find_by_call_id(&self, _call_id: &str) -> Option<ChildStatusRecord> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct DelegationConfig {
    pub enabled: bool,
    /// Max chain depth a *new* delegation may exist at. With `depth_limit = 2`
    /// the chain root → child → grandchild is allowed; the grandchild trying
    /// to spawn a great-grandchild is rejected. See spec §5.
    pub depth_limit: u32,
    /// Per-agent overrides applied when spawning a delegation child. Keyed by
    /// the target `agent_type`; a missing mode uses the product-owned automatic
    /// mode while config values remain unset. Forwarded to
    /// `ConnectionSpawner::spawn` as `preferred_mode_id` /
    /// `preferred_config_values`.
    pub agent_defaults: BTreeMap<AgentType, AgentDelegationDefaults>,
    /// Per-parent byte budget for cached completed-task result text. `0`
    /// disables eviction (unlimited). Surfaced from the settings page in MB and
    /// converted to bytes in `into_broker_config`. Pushed into the pending-calls
    /// bucket by `set_config` so `insert_completed` reads it lock-free.
    pub completed_cache_cap_bytes: usize,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            depth_limit: 1,
            agent_defaults: BTreeMap::new(),
            completed_cache_cap_bytes: DEFAULT_COMPLETED_CACHE_CAP_BYTES,
        }
    }
}

/// A delegation task running in the background after `start_delegation`
/// returned its `Running` ack. The async redesign drops the parked
/// `oneshot::Sender` the old `PendingCall` carried: the parent's
/// `delegate_to_agent` no longer blocks on a channel, so there is nothing to
/// signal. A terminal event instead migrates the entry into `completed` (same
/// lock) and wakes any `get_delegation_status` long-poll via the broker's
/// `result_notify`.
#[derive(Clone)]
struct RunningTask {
    child_connection_id: String,
    child_conversation_id: i32,
    parent_connection_id: String,
    parent_tool_use_id: String,
    /// Target agent — surfaced in status reports.
    agent_type: AgentType,
    /// MCP-side opaque handle minted by the companion per `tools/call`. The
    /// listener forwards it through `DelegationRequest`; we keep it here so
    /// `cancel_by_external_handle` can find the entry. `None` for delegations
    /// that didn't come through MCP (tests, future internal callers).
    external_handle: Option<String>,
    /// When the child started running (after `send_prompt` succeeded). Used to
    /// compute a real `duration_ms` at terminal time.
    started_at: Instant,
    /// Serializes a delayed real-id replay with terminal meta/event I/O. Only
    /// present while this task started from a synthetic late binding.
    late_binding_gate: Option<Arc<Mutex<()>>>,
    /// Host concurrency permit held until terminal teardown completes.
    _concurrency_permit: Arc<ConcurrencyPermit>,
}

struct ConcurrencyPermit {
    _permit: OwnedSemaphorePermit,
    root_conversation_id: i32,
}

struct ConcurrencyWait<'a> {
    root_conversation_id: i32,
    inflight_id: u64,
    parent_connection_id: &'a str,
    external_handle: Option<&'a str>,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        tracing::debug!(
            root_conversation_id = self.root_conversation_id,
            "releasing Agent concurrency permit"
        );
    }
}

/// Broker-owned teardown state for a task canceled by its parent. Keeping this
/// in `PendingInner` makes the slow I/O retryable after the caller's future is
/// dropped (or a detached worker is aborted).
#[derive(Clone)]
struct CanceledTeardown {
    task: RunningTask,
    duration_ms: u64,
    phase: TeardownPhase,
    processing: Arc<Mutex<()>>,
}

#[derive(Clone, Copy)]
enum TeardownPhase {
    Meta,
    Event,
    Cancel,
    Disconnect,
}

/// Wakes a competing/retry worker whenever the current worker releases its
/// per-task processing guard, including panic and task-abort paths.
struct TeardownProcessingLease {
    _processing: tokio::sync::OwnedMutexGuard<()>,
    notify: Arc<Notify>,
}

impl Drop for TeardownProcessingLease {
    fn drop(&mut self) {
        self.notify.notify_waiters();
    }
}

/// A terminal delegation result retained so `get_delegation_status` /
/// `cancel_delegation` can answer after the lifecycle resolved the task.
/// Parent-scoped, FIFO-evicted once the parent's retained result text exceeds
/// `PendingInner::completed_cap_bytes`, and dropped wholesale when the parent
/// connection tears down.
#[derive(Clone)]
struct CompletedTask {
    parent_connection_id: String,
    child_conversation_id: i32,
    agent_type: AgentType,
    status: TaskStatus,
    /// Result text for `Completed` (capped at [`COMPLETED_TEXT_CAP`]). `None`
    /// for failures/cancels.
    text: Option<String>,
    error_code: Option<String>,
    message: Option<String>,
    duration_ms: u64,
}

#[derive(Default)]
struct PendingCalls {
    inner: Mutex<PendingInner>,
}

/// Everything guarded by the single pending-calls mutex. Co-locating the parked
/// calls with the early-terminal bookkeeping under ONE lock is what makes the
/// terminal-vs-registration race safe: a terminal event for a delegation that
/// is still mid-setup (its `handle_request` hasn't parked the [`PendingCall`]
/// yet) and the matching registration are serialized on this lock, so the
/// terminal event either finds the parked entry (resolves via `tx`) or buffers
/// its outcome (and `handle_request` drains it the instant it parks) — never
/// both, never neither. Without this, a terminal that fires in the spawn→park
/// window would no-op the resolver and then strand the parked `rx.await`.
///
/// Both CHILD-terminal pre-park resolvers are covered, because either can win
/// the race against the parent `write_meta` await between `send_prompt` and the
/// park:
///   * `complete_call` — a fast/empty turn's `TurnComplete` (the prompt is only
///     *enqueued* by `send_prompt`; the child loop emits `TurnComplete`
///     independently). Keyed by `call_id`.
///   * `cancel_by_child_connection` — a freshly-spawned child connection dying
///     before its first prompt is answered. Keyed by `child_connection_id`.
///
/// Parent-side cancels (`cancel_by_parent` / `cancel_by_parent_turn`) are
/// covered symmetrically by the `inflight` registry: `handle_request` registers
/// each setup at entry, and `mark_inflight_canceled_for_parent` runs in the SAME
/// lock acquisition that drains the parked `calls`. A parent cancel landing
/// while a child is still mid-setup therefore flags the in-flight record, and
/// `handle_request` observes the flag at its next checkpoint (or atomically at
/// park) and tears the child down itself — it is no longer left to the child's
/// own terminal / connection-teardown cascade.
///
/// The reservation records the `child_connection_id` each resolver gates on;
/// `handle_request` drains both buffers at park.
#[derive(Default)]
struct PendingInner {
    /// Tasks running in the background after their `Running` ack, keyed by
    /// broker `call_id` (= `task_id`). A terminal event migrates an entry from
    /// here into `completed` under THIS lock (atomic `running` → `completed`
    /// transition), so a concurrent `get_delegation_status` never observes a
    /// task as neither running nor completed.
    running: HashMap<String, RunningTask>,
    /// Terminal results retained for `get_delegation_status` / `cancel_delegation`,
    /// keyed by `task_id`. Bounded by the per-parent byte valve
    /// (`completed_cap_bytes` over `completed_bytes`, FIFO-evicted via
    /// `completed_order`) and dropped per-parent on connection teardown.
    /// Evicted/unknown tasks fall back to the DB status lookup.
    completed: HashMap<String, CompletedTask>,
    /// Per-parent FIFO index over `completed` for byte-valve eviction and
    /// per-parent teardown. Keyed by `parent_connection_id`; each deque holds
    /// that parent's completed `task_id`s oldest-first.
    completed_order: HashMap<String, VecDeque<String>>,
    /// Per-parent running total of retained completed result-text bytes (the
    /// `CompletedTask::text` lengths). Drives the `completed_cap_bytes` valve in
    /// `insert_completed`; kept in sync on insert/evict and cleared per-parent
    /// on teardown.
    completed_bytes: HashMap<String, usize>,
    /// Task ids whose terminal projection must survive completed-cache eviction
    /// until a delayed ACP tool_call id is either bound or canceled.
    late_binding_pins: HashSet<String>,
    late_binding_terminals: HashMap<String, LateCompletionProjection>,
    /// Per-parent byte budget for retained completed result text. `0` =
    /// unlimited (no eviction). Seeded by `set_config` from the live
    /// `DelegationConfig` (default until then: `0`, but `set_config` always runs
    /// at startup via `apply_persisted_config`). Read lock-free by
    /// `insert_completed`, which already holds THIS mutex — so the cap is
    /// consulted WITHOUT nesting the `config` lock under the pending lock.
    completed_cap_bytes: usize,
    /// In-setup delegations (spawned + id minted, not yet parked), mapping
    /// `call_id` → `child_connection_id`. Gating the early buffers on membership
    /// here distinguishes a genuine pre-registration race (still reserved →
    /// buffer) from the normal post-resolution teardown that fires on every
    /// completion (no longer reserved → ignore). Removed at park / on the
    /// send-failure path.
    setups: HashMap<String, String>,
    /// Completion outcomes captured by a `TurnComplete` that beat registration
    /// (gated by `setups`), keyed by `call_id`. Each carries the `seq` arrival
    /// stamp taken when it buffered, so the park can order it against a racing
    /// parent cancel (first-terminal-wins). Drained at park.
    early_completes: HashMap<String, (u64, DelegationOutcome)>,
    /// Cancel reasons captured by a child failure that beat registration (gated
    /// by `setups`), keyed by `child_connection_id`. The value pairs the `seq`
    /// arrival stamp (for the park's first-terminal-wins ordering against a
    /// racing parent cancel) with the pre-computed `Canceled { reason }` text
    /// (same wording the parked `cancel_by_child_connection` path produces);
    /// `handle_request` rebuilds the full outcome at park with the real
    /// `child_conversation_id` (which the resolver, finding no entry, lacked).
    early_cancels: HashMap<String, (u64, String)>,
    /// In-flight `handle_request` setups, keyed by a unique per-call id and
    /// registered at entry (BEFORE the claim poll, so the whole claim→park
    /// window is covered). This is the parent-cancel counterpart to `setups`:
    /// `setups` lets a *child* terminal reach a not-yet-parked delegation,
    /// while `inflight` lets a *parent* cancel reach one. `cancel_by_parent*`
    /// flags every entry it owns (`mark_inflight_canceled_for_parent`);
    /// `handle_request` consults the flag after claim, after spawn, and
    /// atomically at park, tearing the spawned child down itself when set.
    /// Removed at park and on every early-return (no Drop guard — see
    /// `register_inflight`).
    inflight: HashMap<u64, InflightSetup>,
    /// Monotonic arrival clock (see `tick`). Hands out the unique `inflight`
    /// keys AND the arrival stamps on buffered child terminals / parent cancels,
    /// so the park can resolve a setup-window race by true first-terminal-wins
    /// order. Keys and stamps share this sequence but are never cross-compared
    /// (keys match by identity, stamps only by `<` against other stamps).
    seq: u64,
    /// Parent-canceled tasks remain here until every terminal projection and
    /// child teardown step has completed. The queue is broker-owned rather than
    /// held by the caller or a detached worker, so a later cancel can retry it.
    canceled_teardowns: HashMap<String, CanceledTeardown>,
}

/// One in-flight `handle_request` setup tracked for parent-cancel coverage.
struct InflightSetup {
    parent_connection_id: String,
    /// `Some(stamp)` once a parent cancel lands while this delegation is
    /// mid-setup (spawned / sending, not yet parked), where `stamp` is the `seq`
    /// arrival-clock value at that moment. First-write-wins and never cleared,
    /// so a cancel can't be lost between `handle_request`'s checkpoints, and its
    /// stamp lets the park order it against a racing child terminal.
    canceled_at: Option<u64>,
}

impl PendingInner {
    /// Mark a delegation as setting-up (spawned + id minted, not yet parked) so
    /// a terminal event racing the park is buffered rather than dropped.
    ///
    /// No cap: a reservation lives only for the brief spawn→park window and is
    /// always released by `unreserve` on every `handle_request` exit (park, or
    /// the send-failure path), so `setups` is bounded by the count of
    /// concurrently-in-setup delegations — it never accumulates stale entries.
    /// A cap here would be actively unsafe: every reservation is live, so
    /// evicting one to make room would drop a real in-flight delegation's race
    /// guard and reopen the very hang this machinery exists to prevent.
    fn reserve(&mut self, call_id: &str, child_connection_id: &str) {
        self.setups
            .insert(call_id.to_string(), child_connection_id.to_string());
    }

    /// Release a delegation's reservation and discard any un-drained buffered
    /// terminal — called once the entry is parked (the buffers were already
    /// drained, so the removals are no-ops then) or when setup errors out
    /// (discarding a buffer no `handle_request` will pick up).
    fn unreserve(&mut self, call_id: &str, child_connection_id: &str) {
        self.setups.remove(call_id);
        self.early_completes.remove(call_id);
        self.early_cancels.remove(child_connection_id);
    }

    /// Whether a child connection belongs to a still-in-setup delegation. O(n)
    /// over `setups`, but n is the (tiny) count of concurrently-in-setup
    /// delegations.
    fn is_child_reserved(&self, child_connection_id: &str) -> bool {
        self.setups
            .values()
            .any(|child| child == child_connection_id)
    }

    /// Buffer a completion for a still-reserved delegation, stamped with the
    /// current arrival clock so the park can order it against a racing parent
    /// cancel. No-op when the `call_id` isn't reserved (already resolved by
    /// another terminal path), so the buffer only ever holds genuine
    /// pre-registration races.
    fn buffer_early_complete(&mut self, call_id: &str, outcome: DelegationOutcome) {
        if self.setups.contains_key(call_id) {
            let stamp = self.tick();
            self.early_completes
                .insert(call_id.to_string(), (stamp, outcome));
        }
    }

    /// Buffer a child failure for a still-reserved delegation, stamped with the
    /// current arrival clock so the park can order it against a racing parent
    /// cancel. No-op when the child isn't reserved (normal post-resolution
    /// teardown). Stores the pre-computed cancel reason so the park rebuilds the
    /// same wording the parked `cancel_by_child_connection` path produces.
    fn buffer_child_failure(&mut self, child_connection_id: &str, detail: Option<String>) {
        if self.is_child_reserved(child_connection_id) {
            let stamp = self.tick();
            self.early_cancels.insert(
                child_connection_id.to_string(),
                (stamp, child_canceled_reason(detail.as_deref())),
            );
        }
    }

    /// Drain a buffered completion with its arrival stamp (by `call_id`) — used
    /// by `handle_request` at park.
    fn take_early_complete(&mut self, call_id: &str) -> Option<(u64, DelegationOutcome)> {
        self.early_completes.remove(call_id)
    }

    /// Drain a buffered cancel reason with its arrival stamp (by
    /// `child_connection_id`) — used by `handle_request` at park.
    fn take_early_cancel(&mut self, child_connection_id: &str) -> Option<(u64, String)> {
        self.early_cancels.remove(child_connection_id)
    }

    /// Advance the monotonic arrival clock, returning the pre-increment value.
    /// Strictly increasing (wraps only after 2^64 calls — unreachable), so two
    /// events stamped under this lock always compare in their true arrival
    /// order. Backs both `inflight` keys and terminal/cancel arrival stamps; the
    /// two uses never cross-compare (keys match by identity, stamps by `<`).
    fn tick(&mut self) -> u64 {
        let v = self.seq;
        self.seq = self.seq.wrapping_add(1);
        v
    }

    fn enqueue_canceled_teardown(&mut self, call_id: String, task: RunningTask, duration_ms: u64) {
        self.canceled_teardowns
            .entry(call_id)
            .or_insert(CanceledTeardown {
                task,
                duration_ms,
                phase: TeardownPhase::Meta,
                processing: Arc::new(Mutex::new(())),
            });
    }

    fn enqueue_turn_canceled_tasks(&mut self, keys: Vec<String>) {
        let drained = drain_and_record_canceled(self, keys.clone(), "parent canceled");
        for (call_id, (task, duration_ms)) in keys.into_iter().zip(drained) {
            self.enqueue_canceled_teardown(call_id, task, duration_ms);
        }
    }

    fn enqueue_connection_canceled_tasks(&mut self, parent_connection_id: &str, keys: Vec<String>) {
        for call_id in keys {
            let task = self.running.remove(&call_id).expect("key just observed");
            let duration_ms = task.started_at.elapsed().as_millis() as u64;
            if self.late_binding_pins.contains(&call_id) {
                let completed = build_completed(
                    &task.parent_connection_id,
                    task.child_conversation_id,
                    task.agent_type,
                    duration_ms,
                    &canceled_outcome(task.child_conversation_id, "parent canceled"),
                );
                if let Some(projection) = late_completion_projection(&completed) {
                    self.late_binding_terminals
                        .insert(call_id.clone(), projection);
                }
            } else {
                self.clear_late_binding_state(&call_id);
            }
            self.enqueue_canceled_teardown(call_id, task, duration_ms);
        }
        self.drop_completed_for_parent(parent_connection_id);
    }

    /// Claim one queued teardown for `parent_connection_id`. The processing
    /// mutex serializes retries. Its owned guard resets automatically if the
    /// worker future is canceled before the current phase is committed.
    fn claim_canceled_teardown(
        &mut self,
        parent_connection_id: &str,
    ) -> Option<(String, CanceledTeardown, tokio::sync::OwnedMutexGuard<()>)> {
        let call_ids: Vec<String> = self
            .canceled_teardowns
            .iter()
            .filter(|(_, teardown)| teardown.task.parent_connection_id == parent_connection_id)
            .map(|(call_id, _)| call_id.clone())
            .collect();
        for call_id in call_ids {
            let Some(teardown) = self.canceled_teardowns.get(&call_id) else {
                continue;
            };
            let Ok(processing) = Arc::clone(&teardown.processing).try_lock_owned() else {
                continue;
            };
            return Some((call_id, teardown.clone(), processing));
        }
        None
    }

    fn has_canceled_teardown_for_parent(&self, parent_connection_id: &str) -> bool {
        self.canceled_teardowns
            .values()
            .any(|teardown| teardown.task.parent_connection_id == parent_connection_id)
    }

    fn advance_canceled_teardown(&mut self, call_id: &str, phase: TeardownPhase) {
        if let Some(teardown) = self.canceled_teardowns.get_mut(call_id) {
            teardown.phase = phase;
        }
    }

    fn finish_canceled_teardown(&mut self, call_id: &str) {
        self.canceled_teardowns.remove(call_id);
    }

    /// Register an in-flight setup at `handle_request` entry, returning its
    /// unique id. The caller MUST `deregister_inflight` on every exit path
    /// (each early-return, and at park). There is deliberately NO Drop guard:
    /// the park hand-off — `calls.insert` followed by `deregister_inflight` —
    /// has to be atomic under this lock so a concurrent parent cancel sees the
    /// entry in exactly one of `inflight` or `calls`, and a guard firing after
    /// the lock releases would reopen that window.
    fn register_inflight(&mut self, parent_connection_id: &str) -> u64 {
        let id = self.tick();
        self.inflight.insert(
            id,
            InflightSetup {
                parent_connection_id: parent_connection_id.to_string(),
                canceled_at: None,
            },
        );
        id
    }

    /// Drop an in-flight setup record (idempotent).
    fn deregister_inflight(&mut self, id: u64) {
        self.inflight.remove(&id);
    }

    /// Whether a parent cancel flagged this in-flight setup. False once the
    /// record is gone (already parked / deregistered). Used by the pre-spawn /
    /// post-spawn checkpoints, which only need the boolean.
    fn inflight_canceled(&self, id: u64) -> bool {
        self.inflight
            .get(&id)
            .map(|s| s.canceled_at.is_some())
            .unwrap_or(false)
    }

    /// Arrival stamp of the parent cancel that flagged this in-flight setup, if
    /// any (`None` when not canceled, or the record is already gone). Used at
    /// park to order the cancel against a buffered child terminal.
    fn inflight_canceled_at(&self, id: u64) -> Option<u64> {
        self.inflight.get(&id).and_then(|s| s.canceled_at)
    }

    /// Flag every in-flight setup owned by `parent_connection_id` as canceled,
    /// stamping each with one shared arrival-clock value (this cancel is a
    /// single event). First-write-wins per setup, so a later cancel can't push
    /// an earlier one's stamp forward. Called from `drain_for_parent_cancel` in
    /// the SAME lock acquisition that drains the parked `calls`, so each of the
    /// parent's delegations is caught either here (still in-flight → flagged;
    /// `handle_request` tears its child down at the next checkpoint) or by the
    /// parked-call drain (already parked) — never neither.
    fn mark_inflight_canceled_for_parent(&mut self, parent_connection_id: &str) {
        let stamp = self.tick();
        for setup in self.inflight.values_mut() {
            if setup.parent_connection_id == parent_connection_id && setup.canceled_at.is_none() {
                setup.canceled_at = Some(stamp);
            }
        }
    }

    /// Insert a terminal result into the completed-cache, then FIFO-evict this
    /// parent's OLDEST results until its retained result-text bytes fit
    /// `completed_cap_bytes` (`0` = unlimited). Evicted tasks fall back to the
    /// DB status lookup (status only — child text lives in the child session).
    /// The just-inserted entry is never the victim: a single result is capped
    /// at [`COMPLETED_TEXT_CAP`] (256 KiB), far below any MB-scale budget, so
    /// the newest result always survives for the LLM's immediate
    /// `get_delegation_status`. The caller does the atomic `running.remove` +
    /// this insert under one lock, then notifies long-poll waiters AFTER
    /// releasing the lock.
    fn insert_completed(&mut self, call_id: &str, task: CompletedTask) {
        let parent = task.parent_connection_id.clone();
        let task_bytes = task.text.as_ref().map_or(0, |t| t.len());
        if self.late_binding_pins.contains(call_id) {
            if let Some(projection) = late_completion_projection(&task) {
                self.late_binding_terminals
                    .insert(call_id.to_string(), projection);
            }
        }
        self.completed.insert(call_id.to_string(), task);
        *self.completed_bytes.entry(parent.clone()).or_insert(0) += task_bytes;
        self.completed_order
            .entry(parent.clone())
            .or_default()
            .push_back(call_id.to_string());
        self.evict_completed_over_cap(&parent);
    }

    fn clear_late_binding_state(&mut self, call_id: &str) {
        self.late_binding_pins.remove(call_id);
        self.late_binding_terminals.remove(call_id);
    }

    /// Evict `parent`'s OLDEST completed results until its retained result-text
    /// bytes fit `completed_cap_bytes` (`0` = unlimited). Evicted tasks fall
    /// back to the DB status lookup (status only — child text lives in the child
    /// session). The newest entry is never evicted: a single result is capped at
    /// [`COMPLETED_TEXT_CAP`] (256 KiB), far below any MB-scale budget, so the
    /// LLM's immediate `get_delegation_status` always hits.
    fn evict_completed_over_cap(&mut self, parent: &str) {
        let cap = self.completed_cap_bytes;
        if cap == 0 {
            return;
        }
        loop {
            if self.completed_bytes.get(parent).copied().unwrap_or(0) <= cap {
                break;
            }
            let evicted = match self.completed_order.get_mut(parent) {
                Some(order) if order.len() > 1 => order.pop_front(),
                _ => None,
            };
            let Some(evicted) = evicted else {
                break;
            };
            if let Some(removed) = self.completed.remove(&evicted) {
                let freed = removed.text.as_ref().map_or(0, |t| t.len());
                if let Some(slot) = self.completed_bytes.get_mut(parent) {
                    *slot = slot.saturating_sub(freed);
                }
            }
        }
    }

    /// Re-apply the current `completed_cap_bytes` to EVERY parent. Called by
    /// `set_config` when the cap may have been LOWERED at runtime, so
    /// already-retained results are pruned promptly — insert-time eviction alone
    /// would otherwise strand them until a parent's next completion (which may
    /// never arrive).
    fn enforce_completed_cap_all_parents(&mut self) {
        if self.completed_cap_bytes == 0 {
            return;
        }
        let parents: Vec<String> = self.completed_bytes.keys().cloned().collect();
        for parent in parents {
            self.evict_completed_over_cap(&parent);
        }
    }

    /// Forget every completed result for a parent. Called on connection
    /// teardown (the parent is gone — nothing left to query). A turn cancel
    /// deliberately does NOT call this: the connection stays alive and the LLM
    /// may still query its just-canceled tasks.
    fn drop_completed_for_parent(&mut self, parent_connection_id: &str) {
        self.completed_bytes.remove(parent_connection_id);
        if let Some(ids) = self.completed_order.remove(parent_connection_id) {
            for id in ids {
                self.completed.remove(&id);
                if !self.late_binding_pins.contains(&id) {
                    self.clear_late_binding_state(&id);
                }
            }
        }
        let projected_ids: Vec<String> = self
            .late_binding_terminals
            .iter()
            .filter(|(_, projection)| projection.parent_connection_id == parent_connection_id)
            .map(|(call_id, _)| call_id.clone())
            .collect();
        for call_id in projected_ids {
            if !self.late_binding_pins.contains(&call_id) {
                self.clear_late_binding_state(&call_id);
            }
        }
    }
}

/// Cap result text retained in the completed-cache. The full output always
/// lives in the child session; this only bounds the broker's copy.
fn cap_completed_text(text: &str) -> String {
    truncate_on_char_boundary(text, COMPLETED_TEXT_CAP)
}

/// Build the bounded inline preview carried by the `DelegationCompleted` event
/// and terminal meta. `None` for empty text.
fn build_text_preview(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    Some(truncate_on_char_boundary(text, STATUS_PREVIEW_CAP))
}

/// Truncate `s` so the RESULT (including the appended ellipsis) is at most `cap`
/// bytes, cut on a UTF-8 char boundary. Reserving the ellipsis bytes keeps the
/// output within the advertised cap rather than `cap + 3`.
fn truncate_on_char_boundary(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    const ELLIPSIS: &str = "…";
    // Leave room for the ellipsis; clamp at 0 for pathologically small caps.
    let budget = cap.saturating_sub(ELLIPSIS.len());
    let mut end = budget.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ELLIPSIS}", &s[..end])
}

/// Derive the completed-cache fields (status / text / error_code / message)
/// from a resolved [`DelegationOutcome`]. `Canceled`-coded errors map to
/// [`TaskStatus::Canceled`]; every other error maps to [`TaskStatus::Failed`].
fn terminal_fields(
    outcome: &DelegationOutcome,
) -> (TaskStatus, Option<String>, Option<String>, Option<String>) {
    match outcome {
        DelegationOutcome::Ok(ok) => (
            TaskStatus::Completed,
            Some(cap_completed_text(&ok.text)),
            None,
            None,
        ),
        DelegationOutcome::Err { code, message, .. } => {
            let status = if code == "canceled" {
                TaskStatus::Canceled
            } else {
                TaskStatus::Failed
            };
            (status, None, Some(code.clone()), Some(message.clone()))
        }
    }
}

/// Build a [`CompletedTask`] from a resolved outcome for the completed-cache.
fn build_completed(
    parent_connection_id: &str,
    child_conversation_id: i32,
    agent_type: AgentType,
    duration_ms: u64,
    outcome: &DelegationOutcome,
) -> CompletedTask {
    let (status, text, error_code, message) = terminal_fields(outcome);
    CompletedTask {
        parent_connection_id: parent_connection_id.to_string(),
        child_conversation_id,
        agent_type,
        status,
        text,
        error_code,
        message,
        duration_ms,
    }
}

/// A `canceled`-coded [`DelegationOutcome`] carrying the child conversation id.
fn canceled_outcome(child_conversation_id: i32, reason: &str) -> DelegationOutcome {
    DelegationOutcome::from_err(
        DelegationError::Canceled {
            reason: reason.to_string(),
        },
        Some(child_conversation_id),
    )
}

/// Remove `keys` from `running`, recording each as a `Canceled` completed entry
/// (so a `get_delegation_status` still answers) and returning the drained tasks
/// — each paired with the `duration_ms` captured at this drain point — for I/O
/// teardown. MUST be called with the pending lock held so the running →
/// completed migration is atomic.
///
/// The duration is captured ONCE here and returned so the slow teardown
/// (parent-card meta, report) reuses the exact value recorded into the
/// completed-cache, rather than recomputing `started_at.elapsed()` later — which
/// would inflate it for the backgrounded `cancel_by_parent_turn` teardown and
/// disagree with the `get_delegation_status` / `cancel_delegation` cards.
fn drain_and_record_canceled(
    inner: &mut PendingInner,
    keys: Vec<String>,
    reason: &str,
) -> Vec<(RunningTask, u64)> {
    let mut out = Vec::with_capacity(keys.len());
    for k in keys {
        let task = inner.running.remove(&k).expect("key just observed");
        let outcome = canceled_outcome(task.child_conversation_id, reason);
        let duration_ms = task.started_at.elapsed().as_millis() as u64;
        inner.insert_completed(
            &k,
            build_completed(
                &task.parent_connection_id,
                task.child_conversation_id,
                task.agent_type,
                duration_ms,
                &outcome,
            ),
        );
        out.push((task, duration_ms));
    }
    out
}

/// Project a `DelegationOutcome` + broker-measured `duration_ms` onto the
/// wire-stable `DelegationResultSummary` carried by `DelegationCompleted`.
/// Keeps the mapping (and the bounded `text_preview`) in one place.
fn outcome_to_summary(outcome: &DelegationOutcome, duration_ms: u64) -> DelegationResultSummary {
    match outcome {
        DelegationOutcome::Ok(ok) => DelegationResultSummary::Ok {
            duration_ms,
            text_preview: build_text_preview(&ok.text),
        },
        DelegationOutcome::Err { code, .. } => DelegationResultSummary::Err {
            error_code: code.clone(),
        },
    }
}

#[derive(Clone)]
struct LateCompletionProjection {
    parent_connection_id: String,
    status: &'static str,
    error_code: Option<String>,
    preview: Option<String>,
    duration_ms: u64,
    result: DelegationResultSummary,
}

fn late_completion_projection(completed: &CompletedTask) -> Option<LateCompletionProjection> {
    match completed.status {
        TaskStatus::Completed => {
            let preview = completed.text.as_deref().and_then(build_text_preview);
            Some(LateCompletionProjection {
                parent_connection_id: completed.parent_connection_id.clone(),
                status: "completed",
                error_code: None,
                preview: preview.clone(),
                duration_ms: completed.duration_ms,
                result: DelegationResultSummary::Ok {
                    duration_ms: completed.duration_ms,
                    text_preview: preview,
                },
            })
        }
        TaskStatus::Failed | TaskStatus::Canceled => {
            let code = completed
                .error_code
                .clone()
                .unwrap_or_else(|| "subagent_error".to_string());
            Some(LateCompletionProjection {
                parent_connection_id: completed.parent_connection_id.clone(),
                status: "failed",
                error_code: Some(code.clone()),
                preview: None,
                duration_ms: completed.duration_ms,
                result: DelegationResultSummary::Err { error_code: code },
            })
        }
        TaskStatus::Running | TaskStatus::Unknown => None,
    }
}

/// Project a resolved outcome onto a terminal [`DelegationTaskReport`] (used by
/// the setup-window terminal dispositions and the test shim).
fn report_from_outcome(
    task_id: Option<String>,
    agent_type: Option<AgentType>,
    outcome: &DelegationOutcome,
    duration_ms: Option<u64>,
) -> DelegationTaskReport {
    let (status, text, error_code, message) = terminal_fields(outcome);
    let child_conversation_id = match outcome {
        DelegationOutcome::Ok(ok) => Some(ok.child_conversation_id),
        DelegationOutcome::Err {
            child_conversation_id,
            ..
        } => *child_conversation_id,
    };
    DelegationTaskReport {
        task_id,
        status,
        child_conversation_id,
        agent_type,
        text,
        error_code,
        message,
        duration_ms,
    }
}

/// Build a `Failed`/`Canceled` report for a setup error (no task id — setup
/// failed before/around registration, so the LLM has no task to track).
fn report_err(
    agent_type: AgentType,
    err: DelegationError,
    child_conversation_id: Option<i32>,
) -> DelegationTaskReport {
    let outcome = DelegationOutcome::from_err(err, child_conversation_id);
    report_from_outcome(None, Some(agent_type), &outcome, None)
}

/// The `Running` ack returned by `start_delegation` for a backgrounded task.
fn running_ack(
    call_id: String,
    child_conversation_id: i32,
    agent_type: AgentType,
) -> DelegationTaskReport {
    // Embed the literal task_id in the message so it survives clients that only
    // surface the MCP `content` text (not `structuredContent`) — without it the
    // LLM couldn't call get_delegation_status / cancel_delegation.
    let message = format!("{DELEGATION_ACK_PREFIX}{call_id}{DELEGATION_ACK_SUFFIX}");
    DelegationTaskReport {
        task_id: Some(call_id),
        status: TaskStatus::Running,
        child_conversation_id: Some(child_conversation_id),
        agent_type: Some(agent_type),
        text: None,
        error_code: None,
        message: Some(message),
        duration_ms: None,
    }
}

/// How long [`DelegationBroker::get_task_status`] may block before returning the
/// current (possibly still-running) snapshot. Derived by the listener from the
/// MCP tool's `wait_ms`: omitted → [`Immediate`], an explicit `0` → [`Infinite`],
/// any positive value → [`Bounded`] (clamped to the listener's hard ceiling).
///
/// [`Immediate`]: StatusWait::Immediate
/// [`Bounded`]: StatusWait::Bounded
/// [`Infinite`]: StatusWait::Infinite
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusWait {
    /// Return the current snapshot right away — the default poll.
    Immediate,
    /// Block up to this many milliseconds, then return whatever snapshot we have
    /// (the child keeps running past the deadline; the caller re-issues to wait
    /// more).
    Bounded(u64),
    /// Block until the task reaches a terminal state — never time out. Lets a
    /// long-running child be awaited in a single call. A parent disconnect or
    /// cancel also drives the task terminal (and fires the completion signal),
    /// so this never outlives the task itself.
    Infinite,
}

/// Status report for a still-running task.
fn running_report(task_id: &str, task: &RunningTask) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: Some(task_id.to_string()),
        status: TaskStatus::Running,
        child_conversation_id: Some(task.child_conversation_id),
        agent_type: Some(task.agent_type),
        text: None,
        error_code: None,
        // Bare baseline; `get_task_status` upgrades this to a two-line
        // "Running.\nLatest sub-agent reply: …" when the child has live output.
        message: Some("Running.".to_string()),
        duration_ms: None,
    }
}

/// Status report from a cached completed result.
fn completed_report(task_id: &str, c: &CompletedTask) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: Some(task_id.to_string()),
        status: c.status,
        child_conversation_id: Some(c.child_conversation_id),
        agent_type: Some(c.agent_type),
        text: c.text.clone(),
        error_code: c.error_code.clone(),
        message: c.message.clone(),
        duration_ms: Some(c.duration_ms),
    }
}

/// Status report when a task id isn't known to the caller (never existed,
/// owned by a different parent, or evicted with no DB record).
fn unknown_report(task_id: &str) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: Some(task_id.to_string()),
        status: TaskStatus::Unknown,
        child_conversation_id: None,
        agent_type: None,
        text: None,
        error_code: None,
        message: Some(
            "Unknown task id — it never existed, isn't owned by this session, \
             or its result was evicted with no stored record."
                .to_string(),
        ),
        duration_ms: None,
    }
}

/// Status report recovered from the DB after the in-memory result was evicted.
/// Carries status only — the full output lives in the child session.
fn db_report(task_id: &str, rec: &ChildStatusRecord) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: Some(task_id.to_string()),
        status: rec.status,
        child_conversation_id: Some(rec.child_conversation_id),
        agent_type: Some(rec.agent_type),
        text: None,
        error_code: (rec.status == TaskStatus::Canceled).then(|| "canceled".to_string()),
        message: Some(format!(
            "Result no longer cached; open child session {} for the full output.",
            rec.child_conversation_id
        )),
        duration_ms: None,
    }
}

/// Per-id classification captured under the pending lock during a (possibly
/// batched) status query. The async resolution that can't run under the lock —
/// `attach_live_reply` (a different lock) for a running task, `status_from_db`
/// (a DB round-trip) for one not in memory — is deferred to `assemble_reports`
/// AFTER the lock is released, so a status query never nests the pending lock
/// inside another await. This is the same lock-ordering the single-task path
/// has always used; batching just captures it per id.
enum StatusClass {
    /// Terminal/owned-cached, or a cross-parent `unknown` — the report is final.
    Settled(DelegationTaskReport),
    /// Running and owned — the bare running snapshot plus its child connection
    /// id, so `assemble_reports` can attach the latest live reply out of lock.
    Running {
        report: DelegationTaskReport,
        child_connection_id: String,
    },
    /// Neither running nor completed in memory — resolve via the DB fallback in
    /// `assemble_reports`. A not-in-memory id is, for wait purposes, already
    /// settled: it can never transition back to running, so a batch wait need
    /// not park on it (and must not hit the DB on every wake).
    NotInMemory,
}

/// Classify one task id against the in-memory maps while the pending lock is
/// held. Mirrors the single-task resolution order — completed cache (parent
/// scoped) → running set (parent scoped) → not-in-memory — and yields a
/// cross-parent hit as `unknown` so a task owned by another parent never leaks.
fn classify_locked(inner: &PendingInner, parent_connection_id: &str, task_id: &str) -> StatusClass {
    if let Some(c) = inner.completed.get(task_id) {
        if c.parent_connection_id == parent_connection_id {
            return StatusClass::Settled(completed_report(task_id, c));
        }
        return StatusClass::Settled(unknown_report(task_id));
    }
    match inner.running.get(task_id) {
        Some(r) if r.parent_connection_id == parent_connection_id => StatusClass::Running {
            report: running_report(task_id, r),
            child_connection_id: r.child_connection_id.clone(),
        },
        Some(_) => StatusClass::Settled(unknown_report(task_id)),
        None => StatusClass::NotInMemory,
    }
}

/// Map a terminal [`DelegationTaskReport`] back to a [`DelegationOutcome`] for
/// the test-only `handle_request` shim (so pre-async tests keep asserting on
/// the old outcome shape).

/// Build the `Canceled { reason }` string for a child that ended without a
/// clean `TurnComplete`, optionally stitching in the terminal `Error` detail.
/// Shared by `cancel_by_child_connection` and `handle_request`'s early-terminal
/// pickup so both surface the same wording.
fn child_canceled_reason(terminal_error: Option<&str>) -> String {
    match terminal_error {
        Some(detail) if !detail.trim().is_empty() => {
            format!("child session ended without TurnComplete: {detail}")
        }
        _ => "child session ended without TurnComplete".to_string(),
    }
}

/// Set of parent-scoped MCP-side `external_handle` tokens for which the companion
/// already received `notifications/cancelled` BEFORE the matching
/// `handle_request` reached the pending-registration phase. Without
/// this pre-cancel buffer, a fast cancel that lands during the
/// pre-check / spawn window would find no entry in `pending`, drop
/// silently, and let the broker proceed to spawn a child the caller
/// no longer wants. `handle_request` consults this set both at entry
/// (so we never even spawn) and immediately after parking the pending
/// entry (so a cancel landing mid-spawn still wins).
///
/// Capped at [`PRE_CANCELED_CAP`] so a misbehaving MCP client (or a
/// pathological cancel-for-unknown-id storm) can't grow the set
/// without bound. Eviction is FIFO via the parallel `order` deque,
/// which is fine because pre-cancels only matter for the short window
/// between the cancel and the late-arriving `handle_request`.
#[derive(Default)]
struct PreCanceledHandles {
    inner: Mutex<PreCanceledState>,
}

#[derive(Default)]
struct PreCanceledState {
    set: HashSet<(String, String)>,
    order: VecDeque<(String, String)>,
}

const PRE_CANCELED_CAP: usize = 256;

/// Per-parent tracking of `tool_call_id`s that the ACP lifecycle
/// observed firing `delegate_to_agent`. MCP clients (Codex, Claude
/// Code) generally do NOT populate `_meta.tool_use_id` when invoking
/// an MCP tool, so the broker can't read the LLM-issued
/// `tool_use_id` from the wire — we capture it from the parallel ACP
/// `tool_call` event stream instead.
///
/// Each bucket holds two FIFOs under the SAME mutex:
///
/// * `pending` — ids the lifecycle has registered but the matching
///   broker round-trip has not yet claimed. UNKEYED entries are subject
///   to [`PENDING_TOOL_CALL_TTL`] eviction so an anonymous ACP id whose
///   MCP round-trip never arrives can't linger and FIFO-mis-bind a later
///   delegation. KEYED entries carry no count cap: they are drained only
///   by their exact-match claim, by terminal resolution
///   (`resolve_terminal_tool_call_by_task_id`), or by per-parent teardown —
///   because the host may serialize a delegation's round-trip arbitrarily far
///   behind earlier long-running ones, so a count cap would drop a still-pending
///   keyed id and orphan its card.
/// * `consumed` — ids that were already claimed by a prior
///   round-trip. NEITHER subject to TTL eviction NOR to a per-bucket
///   cap: a delegated child agent may run for minutes to hours, and
///   the host can re-emit the same `tool_call` (e.g. as a `completed`
///   status flip) at the end of that run, so the consumed memory
///   must outlast the entire parent-side tool call lifetime. It is
///   scoped to the parent connection's lifetime instead, cleared by
///   `drop_pending_tool_calls_for_parent` on disconnect. The growth
///   is naturally bounded by how many `delegate_to_agent` calls a
///   single parent session issues — typically tens at most, with
///   each `(String, Instant)` entry costing well under 100 bytes —
///   so an unbounded set is comfortable for realistic high-fan-out
///   sessions without OOM risk in the typical operating envelope.
///
/// Co-locating the two halves under one lock makes the
/// claim → mark-consumed pair atomic. A host re-emit racing with the
/// claim cannot observe an empty pending queue AND a consumed memory
/// that does not yet remember the id; consequently it cannot inject
/// a stale duplicate that would mis-bind the next delegation.
#[derive(Default)]
struct ToolCallTracker {
    inner: Mutex<HashMap<String, ToolCallTrackerBucket>>,
}

/// The arguments that uniquely identify a `delegate_to_agent` invocation,
/// used to correlate a parent-side ACP `tool_call` to the matching MCP
/// `tools/call` round-trip. All three fields are values the LLM passed
/// identically to both wire paths, so the triple is the deterministic key
/// when a parent fires several `delegate_to_agent` calls in parallel —
/// matching on `task` alone would swap two calls targeting different agents
/// with the same task, and adding `agent_type` alone would still swap two
/// same-agent/same-task calls aimed at different directories (e.g. "run
/// tests" against `/repo-a` vs `/repo-b`).
///
/// `working_dir` here is the value the LLM EXPLICITLY passed (`None` when
/// omitted), NOT the listener-defaulted spawn directory: the listener
/// defaults a missing MCP `working_dir` to the parent's launch dir, but the
/// ACP `raw_input` omits it then too, so keying on the explicit value keeps
/// both sides symmetric (`None == None`) for the common omitted case while
/// still distinguishing two calls that name different directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationMatchKey {
    pub agent_type: AgentType,
    pub task: String,
    pub working_dir: Option<String>,
}

/// One captured parent-side `delegate_to_agent` tool_call awaiting its
/// matching MCP round-trip.
struct PendingToolCall {
    tool_call_id: String,
    /// The `(agent_type, task, working_dir)` correlation key parsed from the ACP
    /// tool_call's `raw_input`. Matched against the MCP round-trip's own
    /// key so parallel `delegate_to_agent` calls each bind to their own
    /// `tool_call_id` regardless of arrival order — pure arrival-order FIFO
    /// can mis-assign them (or, when one MCP round-trip out-races the
    /// matching ACP event, orphan to a synthetic id). `None` when the host
    /// shipped no parseable `raw_input` at ToolCall time; such entries are
    /// claimable ONLY via the post-budget FIFO fallback
    /// (`take_pending_tool_call`), never the in-loop key-match path.
    match_key: Option<DelegationMatchKey>,
    registered_at: Instant,
}

#[derive(Clone)]
struct LateToolCallBinding {
    call_id: String,
    match_key: DelegationMatchKey,
    parent_connection_id: String,
    parent_conversation_id: i32,
    child_connection_id: String,
    child_conversation_id: i32,
    agent_type: AgentType,
    gate: Arc<Mutex<()>>,
}

enum ToolCallRegistration {
    AlreadyConsumed,
    Duplicate,
    Queued,
    LateBound(LateToolCallBinding),
}

enum TerminalToolCallResolution {
    Ignored,
    Tombstoned,
    LateBound(LateToolCallBinding),
}

#[derive(Default)]
struct ToolCallTrackerBucket {
    pending: VecDeque<PendingToolCall>,
    consumed: VecDeque<(String, Instant)>,
    late_bindings: VecDeque<LateToolCallBinding>,
}

impl ToolCallTrackerBucket {
    fn register(
        &mut self,
        tool_call_id: String,
        match_key: Option<DelegationMatchKey>,
        now: Instant,
    ) -> ToolCallRegistration {
        if self.consumed.iter().any(|(id, _)| id == &tool_call_id) {
            return ToolCallRegistration::AlreadyConsumed;
        }
        let mut duplicate = true;
        if let Some(existing) = self
            .pending
            .iter_mut()
            .find(|pending| pending.tool_call_id == tool_call_id)
        {
            if let Some(key) = match_key {
                if existing.match_key.as_ref() != Some(&key) {
                    existing.match_key = Some(key);
                    duplicate = false;
                }
            }
        } else {
            self.pending.push_back(PendingToolCall {
                tool_call_id: tool_call_id.clone(),
                match_key,
                registered_at: now,
            });
            duplicate = false;
        }
        if let Some(binding) = self.claim_late_binding(&tool_call_id, now) {
            return ToolCallRegistration::LateBound(binding);
        }
        if duplicate {
            ToolCallRegistration::Duplicate
        } else {
            ToolCallRegistration::Queued
        }
    }

    fn arm_late_binding(&mut self, binding: LateToolCallBinding, now: Instant) -> Option<String> {
        if let Some(position) = self
            .pending
            .iter()
            .position(|pending| pending.match_key.as_ref() == Some(&binding.match_key))
        {
            let tool_call_id = self.pending.remove(position)?.tool_call_id;
            self.consumed.push_back((tool_call_id.clone(), now));
            return Some(tool_call_id);
        }
        if !self
            .late_bindings
            .iter()
            .any(|pending| pending.call_id == binding.call_id)
        {
            self.late_bindings.push_back(binding);
        }
        None
    }

    fn claim_late_binding(
        &mut self,
        tool_call_id: &str,
        now: Instant,
    ) -> Option<LateToolCallBinding> {
        let key = self
            .pending
            .iter()
            .find(|pending| pending.tool_call_id == tool_call_id)?
            .match_key
            .as_ref()?;
        let late_position = self
            .late_bindings
            .iter()
            .position(|pending| &pending.match_key == key)?;
        let binding = self.late_bindings.remove(late_position)?;
        let pending_position = self
            .pending
            .iter()
            .position(|pending| pending.tool_call_id == tool_call_id)?;
        self.pending.remove(pending_position);
        self.consumed.push_back((tool_call_id.to_string(), now));
        Some(binding)
    }

    fn resolve_terminal_tool_call(
        &mut self,
        tool_call_id: &str,
        task_id: Option<&str>,
        now: Instant,
    ) -> TerminalToolCallResolution {
        if self.consumed.iter().any(|(id, _)| id == tool_call_id) {
            return TerminalToolCallResolution::Ignored;
        }
        let pending_position = self
            .pending
            .iter()
            .position(|pending| pending.tool_call_id == tool_call_id);
        let binding_position = task_id.and_then(|task_id| {
            self.late_bindings
                .iter()
                .position(|binding| binding.call_id == task_id)
        });
        if let Some(position) = binding_position {
            let Some(binding) = self.late_bindings.remove(position) else {
                return TerminalToolCallResolution::Ignored;
            };
            if let Some(position) = pending_position {
                self.pending.remove(position);
            }
            self.consumed.push_back((tool_call_id.to_string(), now));
            return TerminalToolCallResolution::LateBound(binding);
        }
        if let Some(position) = pending_position {
            self.pending.remove(position);
            self.consumed.push_back((tool_call_id.to_string(), now));
            return TerminalToolCallResolution::Tombstoned;
        }
        TerminalToolCallResolution::Ignored
    }

    fn cancel_late_binding(&mut self, call_id: &str, now: Instant) -> bool {
        let Some(position) = self
            .late_bindings
            .iter()
            .position(|binding| binding.call_id == call_id)
        else {
            return false;
        };
        let Some(binding) = self.late_bindings.remove(position) else {
            return false;
        };
        if let Some(pending_position) = self
            .pending
            .iter()
            .position(|pending| pending.match_key.as_ref() == Some(&binding.match_key))
        {
            if let Some(pending) = self.pending.remove(pending_position) {
                self.consumed.push_back((pending.tool_call_id, now));
            }
        }
        true
    }
}

fn drop_tool_calls_for_parent_locked(
    map: &mut HashMap<String, ToolCallTrackerBucket>,
    parent_connection_id: &str,
    keep_consumed: bool,
) -> Vec<String> {
    if !keep_consumed {
        return map
            .remove(parent_connection_id)
            .map(|bucket| {
                bucket
                    .late_bindings
                    .into_iter()
                    .map(|binding| binding.call_id)
                    .collect()
            })
            .unwrap_or_default();
    }
    let Some(bucket) = map.get_mut(parent_connection_id) else {
        return Vec::new();
    };
    let binding_call_ids = bucket
        .late_bindings
        .iter()
        .map(|binding| binding.call_id.clone())
        .collect();
    let now = Instant::now();
    let cleared: Vec<String> = bucket
        .pending
        .drain(..)
        .map(|pending| pending.tool_call_id)
        .collect();
    for tool_call_id in cleared {
        if !bucket.consumed.iter().any(|(id, _)| id == &tool_call_id) {
            bucket.consumed.push_back((tool_call_id, now));
        }
    }
    bucket.late_bindings.clear();
    if bucket.consumed.is_empty() {
        map.remove(parent_connection_id);
    }
    binding_call_ids
}

/// Maximum age before a `pending` entry is discarded as stale — but ONLY for
/// UNKEYED entries (anonymous, arrival-order correlated). KEYED entries are
/// retained regardless of age: each is claimed solely by an exact key match,
/// so it can't mis-bind a later delegation, and its MCP round-trip may be
/// serialized arbitrarily far behind earlier long-running delegations (Claude
/// Code runs parallel `delegate_to_agent` calls one-at-a-time — observed gap
/// 77 s). See the retain block in `take_matching_tool_call_at`.
/// 60 s comfortably covers the ACP→MCP race for the unkeyed case (<5 ms
/// typical) while still GC'ing a forgotten anonymous id before it can
/// FIFO-mis-bind a subsequent unkeyed delegation.
///
/// The `consumed` side has no TTL — see [`ToolCallTrackerBucket`] — because
/// long-running delegations can re-emit the parent-side `tool_call` well past
/// this window.
const PENDING_TOOL_CALL_TTL: Duration = Duration::from_secs(60);

/// Poll cadence and budget used by `claim_pending_tool_call_with_brief_wait`
/// to correlate an MCP `delegate_to_agent` round-trip to its parent-side
/// ACP `tool_call_id`. The exact-match path returns instantly; this budget is
/// spent while waiting for THIS delegation's own `tool_call` to register (or to
/// backfill its key onto an already-registered entry) so we bind by exact match
/// instead of stealing a parallel sibling's id, or while no claimable id has
/// arrived yet. Unkeyed entries are never claimed in-loop — arrival-order FIFO
/// is deferred to the post-budget last resort, which runs only after the caller
/// has waited the full budget (the correct clock for "this delegation has no
/// key coming"), so a round-trip can't grab a sibling's not-yet-keyed id
/// mid-race.
///
/// 200 × 10 ms = 2 s. This budget only matters when the MCP round-trip
/// out-races its own ACP `tool_call` registration — i.e. the `tools/call`
/// reaches the broker before the in-process `session/update(tool_call)` (and
/// any slightly-later `ToolCallUpdate` carrying the `agent_type`/`task` args)
/// has registered the key. That race is sub-5ms locally; the headroom covers
/// busier hosts and split arg streaming. The wait is invisible in the happy
/// path (it returns the instant the key matches) and negligible against the
/// multi-second-to-minutes child run it precedes.
///
/// NOTE: the budget is NOT what protects a *serialized* second delegation
/// whose round-trip lands many seconds after its tool_call registered (Claude
/// Code runs parallel `delegate_to_agent` calls one-at-a-time, so the 2nd may
/// arrive minutes later). That id is already registered and waiting — the
/// thing that used to orphan it was age-eviction, now fixed by retaining keyed
/// entries indefinitely (see `take_matching_tool_call_at`'s retain
/// block). A host that emits no observable ACP `tool_call` at all still falls
/// through to the synthetic id after the budget, exactly as before.
const CLAIM_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLAIM_POLL_ATTEMPTS: usize = 200;

/// The broker is intentionally `Clone` (cheap — only `Arc`s inside) so
/// listener/handler code can hand copies to spawned tasks without lifetime
/// gymnastics.
#[derive(Clone)]
pub struct DelegationBroker {
    spawner: Arc<dyn ConnectionSpawner>,
    depth_lookup: Arc<dyn ConversationDepthLookup>,
    /// Writer for `meta["iyw-claw.delegation"]` on the parent's active
    /// `delegate_to_agent` ToolCallState. Defaults to a no-op so tests
    /// that aren't exercising the meta lifecycle don't need to wire
    /// anything; production constructs the broker with the
    /// `ConnectionManagerMetaWriter` via `with_writers`.
    meta_writer: Arc<dyn DelegationMetaWriter>,
    /// Emitter for `AcpEvent::DelegationCompleted` against the parent
    /// connection's event stream. Same Noop/Mock/Production scheme as
    /// the meta writer — production wires `ConnectionManagerEventEmitter`
    /// via `with_writers`; tests that don't observe the event lifecycle
    /// take the default Noop.
    event_emitter: Arc<dyn DelegationEventEmitter>,
    /// DB fallback for `get_delegation_status` / `cancel_delegation` once a
    /// task's result aged out of the in-memory completed-cache. Defaults to a
    /// no-op ("unknown"); production wires `DbChildStatusLookup` via
    /// `with_status_lookup`.
    status_lookup: Arc<dyn ChildStatusLookup>,
    /// Peeks a still-running child's live session for a one-line progress hint,
    /// used to enrich `get_delegation_status`'s running report. Defaults to a
    /// no-op ("no hint"); production wires `ConnectionManagerLiveReplyLookup` via
    /// `with_live_reply_lookup`.
    live_reply_lookup: Arc<dyn ChildLiveReplyLookup>,
    pending: Arc<PendingCalls>,
    tool_calls: Arc<ToolCallTracker>,
    pre_canceled_handles: Arc<PreCanceledHandles>,
    config: Arc<Mutex<DelegationConfig>>,
    /// Default capacity used when a root conversation first enters the broker.
    /// Existing root pools intentionally retain their capacity after a setting
    /// change so an in-flight session is never interrupted by a resize.
    concurrency_limit: Arc<Mutex<u32>>,
    concurrency_pools: Arc<Mutex<HashMap<i32, Arc<Semaphore>>>>,
    /// Woken after every terminal `record_completed` so a `get_delegation_status`
    /// long-poll wakes the instant its task finishes instead of busy-polling.
    result_notify: Arc<Notify>,
    /// Coordinates concurrent/retry workers that drain parent-canceled child
    /// teardowns without sharing wakeups with result-status long polls.
    teardown_notify: Arc<Notify>,
}

impl DelegationBroker {
    pub fn new(
        spawner: Arc<dyn ConnectionSpawner>,
        depth_lookup: Arc<dyn ConversationDepthLookup>,
    ) -> Self {
        Self::with_writers(
            spawner,
            depth_lookup,
            Arc::new(NoopMetaWriter) as Arc<dyn DelegationMetaWriter>,
            Arc::new(NoopEventEmitter) as Arc<dyn DelegationEventEmitter>,
        )
    }

    /// Test-only constructor that injects a meta writer but keeps the
    /// default Noop event emitter. Retained so existing meta-focused
    /// tests don't have to mention the emitter parameter. New callsites
    /// (and production wiring) should prefer `with_writers`.
    pub fn with_meta_writer(
        spawner: Arc<dyn ConnectionSpawner>,
        depth_lookup: Arc<dyn ConversationDepthLookup>,
        meta_writer: Arc<dyn DelegationMetaWriter>,
    ) -> Self {
        Self::with_writers(
            spawner,
            depth_lookup,
            meta_writer,
            Arc::new(NoopEventEmitter) as Arc<dyn DelegationEventEmitter>,
        )
    }

    /// Production-grade constructor wiring the broker to both a real
    /// meta writer (`ConnectionManagerMetaWriter`) AND an event emitter
    /// (`ConnectionManagerEventEmitter`). Tests that observe the full
    /// lifecycle (meta writes + DelegationCompleted emits) should use
    /// this with `MockMetaWriter` + `MockEventEmitter`.
    pub fn with_writers(
        spawner: Arc<dyn ConnectionSpawner>,
        depth_lookup: Arc<dyn ConversationDepthLookup>,
        meta_writer: Arc<dyn DelegationMetaWriter>,
        event_emitter: Arc<dyn DelegationEventEmitter>,
    ) -> Self {
        Self {
            spawner,
            depth_lookup,
            meta_writer,
            event_emitter,
            status_lookup: Arc::new(NoopChildStatusLookup),
            live_reply_lookup: Arc::new(NoopChildLiveReplyLookup),
            pending: Arc::new(PendingCalls::default()),
            tool_calls: Arc::new(ToolCallTracker::default()),
            pre_canceled_handles: Arc::new(PreCanceledHandles::default()),
            config: Arc::new(Mutex::new(DelegationConfig::default())),
            concurrency_limit: Arc::new(Mutex::new(
                crate::commands::agent_concurrency::DEFAULT_MAX_CONCURRENT_SUBAGENTS,
            )),
            concurrency_pools: Arc::new(Mutex::new(HashMap::new())),
            result_notify: Arc::new(Notify::new()),
            teardown_notify: Arc::new(Notify::new()),
        }
    }

    /// Replace the DB status fallback used by `get_delegation_status` /
    /// `cancel_delegation` for tasks evicted from the in-memory completed-cache.
    /// Builder-style so the production wiring can layer it onto `with_writers`
    /// without growing that constructor's arity, and tests can opt in.
    pub fn with_status_lookup(mut self, status_lookup: Arc<dyn ChildStatusLookup>) -> Self {
        self.status_lookup = status_lookup;
        self
    }

    /// Replace the live-reply lookup used to enrich `get_delegation_status`'s
    /// running report with the child's latest one-line progress. Builder-style,
    /// layered onto `with_writers` by the production wiring; tests opt in with a
    /// `MockChildLiveReplyLookup`.
    pub fn with_live_reply_lookup(
        mut self,
        live_reply_lookup: Arc<dyn ChildLiveReplyLookup>,
    ) -> Self {
        self.live_reply_lookup = live_reply_lookup;
        self
    }

    /// Record a parent ACP `tool_call_id` whose title indicates the LLM is
    /// invoking `delegate_to_agent`. The next broker round-trip from the
    /// same `parent_connection_id` will claim this id as its
    /// `parent_tool_use_id`. Bounded FIFO per connection.
    ///
    /// Two-tier dedupe against host re-emits of `sessionUpdate(tool_call)`
    /// (some hosts use the non-update variant to ship status flips and
    /// late-arriving `raw_input` chunks):
    ///
    /// 1. **In-queue**: if the id is still waiting to be claimed, drop
    ///    the re-emit — the first push will be consumed by the matching
    ///    MCP round-trip.
    /// 2. **Recently consumed**: if the id was already claimed for an
    ///    earlier delegation on the same parent, drop the re-emit —
    ///    otherwise it would sit in the queue as a stale id and mis-
    ///    bind the **next** delegation's MCP round-trip. The consumed
    ///    memory persists for the parent connection's lifetime (no
    ///    TTL, no cap) so a host re-emit at terminal status flip is
    ///    still rejected even if the delegation ran for hours.
    pub async fn register_pending_tool_call(
        &self,
        parent_connection_id: &str,
        tool_call_id: String,
    ) {
        self.register_pending_tool_call_with_key_at(
            parent_connection_id,
            tool_call_id,
            None,
            Instant::now(),
        )
        .await;
    }

    /// `register_pending_tool_call` that also records the
    /// `(agent_type, task, working_dir)` correlation key parsed from the
    /// tool_call's `raw_input`. The key lets
    /// the broker bind this id to its matching MCP round-trip deterministically
    /// for parallel `delegate_to_agent` calls that pure arrival-order FIFO can
    /// mis-assign. Production registration (from the ACP lifecycle dispatcher)
    /// goes through here.
    pub async fn register_pending_tool_call_with_key(
        &self,
        parent_connection_id: &str,
        tool_call_id: String,
        match_key: Option<DelegationMatchKey>,
    ) {
        self.register_pending_tool_call_with_key_at(
            parent_connection_id,
            tool_call_id,
            match_key,
            Instant::now(),
        )
        .await;
    }

    /// Core registration. Holds the [`ToolCallTracker`] mutex across both
    /// dedupe tiers AND the push so no concurrent `take` can split the
    /// "queue empty + not yet recorded as consumed" window where a host
    /// re-emit could otherwise inject a stale duplicate.
    ///
    /// Two-tier dedupe against host re-emits of `sessionUpdate(tool_call)`
    /// (some hosts use the non-update variant to ship status flips and
    /// late-arriving `raw_input` chunks):
    ///
    /// 1. **Recently consumed**: if the id was already claimed for an
    ///    earlier delegation on the same parent, drop the re-emit —
    ///    otherwise it would sit in the queue as a stale id and mis-bind
    ///    the **next** delegation's MCP round-trip. The consumed memory
    ///    persists for the parent connection's lifetime (no TTL, no cap)
    ///    so a host re-emit at terminal status flip is still rejected
    ///    even if the delegation ran for hours.
    /// 2. **In-queue**: if the id is still waiting to be claimed, drop the
    ///    re-emit rather than push a duplicate — EXCEPT we backfill the
    ///    `match_key` onto an entry registered without one. This is the common
    ///    case for hosts that emit an arg-less initial `ToolCall` and ship the
    ///    `agent_type`/`task` arguments on a following `ToolCallUpdate`: the
    ///    lifecycle dispatcher registers BOTH variants (see
    ///    `register_delegation_tool_call_from_event`), so the first call lands
    ///    here unkeyed and the later update re-enters and back-fills the key.
    ///    Keying the entry this way is what lets it survive past the unkeyed
    ///    GC TTL (see `take_matching_tool_call_at`'s retain block).
    async fn register_pending_tool_call_with_key_at(
        &self,
        parent_connection_id: &str,
        tool_call_id: String,
        match_key: Option<DelegationMatchKey>,
        now: Instant,
    ) {
        let registration = {
            let mut map = self.tool_calls.inner.lock().await;
            map.entry(parent_connection_id.to_string())
                .or_default()
                .register(tool_call_id.clone(), match_key, now)
        };
        match registration {
            ToolCallRegistration::AlreadyConsumed => tracing::info!(
                "[delegation] dropping ACP tool_call_id={tool_call_id} on conn={parent_connection_id} (already consumed by an earlier delegation)"
            ),
            ToolCallRegistration::Duplicate => tracing::info!(
                "[delegation] dropping duplicate ACP tool_call_id={tool_call_id} on conn={parent_connection_id}"
            ),
            ToolCallRegistration::Queued => {}
            ToolCallRegistration::LateBound(binding) => {
                self.apply_late_tool_call_binding(binding, tool_call_id)
                    .await;
            }
        }
    }

    /// Pop the oldest pending `tool_call_id` for the given parent, if any.
    /// Skips entries older than [`PENDING_TOOL_CALL_TTL`] so an ACP id whose
    /// matching MCP round-trip never arrived cannot mis-bind a later
    /// delegation. Mutates the queue in-place; the bucket is removed once
    /// drained.
    pub async fn take_pending_tool_call(&self, parent_connection_id: &str) -> Option<String> {
        self.take_pending_tool_call_at(parent_connection_id, Instant::now())
            .await
    }

    /// `take_pending_tool_call` with an injected "as of" instant. The
    /// public entry point pins it to `Instant::now()`; tests can supply
    /// a future instant to exercise TTL eviction without sleeping past
    /// [`PENDING_TOOL_CALL_TTL`].
    ///
    /// Anonymous claim: returns the oldest *unkeyed* pending id, GC'ing stale
    /// unkeyed entries along the way. KEYED entries are stepped over and left
    /// in place — they're reserved for their exact-key-match round-trip and
    /// must never be handed out by this arrival-order path (doing so would
    /// steal an in-flight delegation's id). Returns `None` when no unkeyed
    /// entry is claimable, even if keyed entries remain.
    async fn take_pending_tool_call_at(
        &self,
        parent_connection_id: &str,
        now: Instant,
    ) -> Option<String> {
        let mut map = self.tool_calls.inner.lock().await;
        let bucket = map.get_mut(parent_connection_id)?;
        // Anonymous claim (post-budget last resort + legacy single-delegation
        // path): only UNKEYED entries are eligible. A keyed entry identifies a
        // specific in-flight delegation and is claimable ONLY by its
        // exact-key-match round-trip; grabbing it here would steal that
        // delegation's id and make IT the dead card. Walk oldest→newest,
        // GC'ing stale unkeyed entries and stepping over keyed ones, until we
        // find the oldest fresh unkeyed id. When only keyed siblings remain we
        // return `None` — the caller then mints a synthetic id rather than
        // mis-binding a sibling.
        let mut claimed: Option<String> = None;
        let mut idx = 0;
        while idx < bucket.pending.len() {
            if bucket.pending[idx].match_key.is_some() {
                idx += 1; // keyed: leave it for its exact-match round-trip
                continue;
            }
            if now.duration_since(bucket.pending[idx].registered_at) > PENDING_TOOL_CALL_TTL {
                if let Some(stale) = bucket.pending.remove(idx) {
                    let age_secs = now.duration_since(stale.registered_at).as_secs();
                    tracing::info!(
                        "[delegation] evicting stale UNKEYED ACP tool_call_id={} (age={age_secs}s) on conn={parent_connection_id}",
                        stale.tool_call_id
                    );
                }
                // `remove` shifted later entries left into `idx`; re-check it.
                continue;
            }
            claimed = bucket.pending.remove(idx).map(|p| p.tool_call_id);
            break;
        }
        // Same mutex span: record the claim into the consumed memory so
        // a concurrent re-register cannot observe "pending empty AND
        // consumed missing" and inject a stale duplicate. Consumed
        // entries persist for the whole parent connection lifetime
        // (no TTL, no cap — see `ToolCallTrackerBucket`) and are only
        // released when the parent disconnects.
        if let Some(id) = &claimed {
            bucket.consumed.push_back((id.clone(), now));
        }
        if bucket.pending.is_empty()
            && bucket.consumed.is_empty()
            && bucket.late_bindings.is_empty()
        {
            map.remove(parent_connection_id);
        }
        claimed
    }

    /// Claim the pending `tool_call_id` for `parent_connection_id` whose
    /// recorded key matches `key` (exact `(agent_type, task, working_dir)`
    /// match). This is the ONLY claim this method makes — it never hands out an
    /// unkeyed entry, because an unkeyed entry may belong to a *different*
    /// parallel delegation whose round-trip simply hasn't registered (or keyed)
    /// its `tool_call` yet, and claiming it by arrival order would steal that
    /// sibling's id. Returns `None` (so the caller keeps polling) whenever no
    /// entry's key matches — whether keyed siblings or only unkeyed entries are
    /// present.
    ///
    /// Arrival-order FIFO for genuinely keyless hosts is deferred to the
    /// post-budget last resort `take_pending_tool_call`, which runs only after
    /// the caller has waited its full budget (see
    /// `claim_pending_tool_call_with_brief_wait`) — the correct clock for "no
    /// key is coming", since a host can serialize a round-trip arbitrarily far
    /// behind its `tool_call` registration, so the entry's own age can never
    /// prove a key won't still arrive. Evicts stale *unkeyed* entries along the
    /// way; keyed entries are retained regardless of age (their round-trip may
    /// be serialized far behind earlier delegations — see the retain block) and
    /// an exact key match claims them at any age.
    pub async fn take_matching_tool_call(
        &self,
        parent_connection_id: &str,
        key: &DelegationMatchKey,
    ) -> Option<String> {
        self.take_matching_tool_call_at(parent_connection_id, key, Instant::now())
            .await
    }

    /// `take_matching_tool_call` with an injected "as of"
    /// instant for TTL tests.
    async fn take_matching_tool_call_at(
        &self,
        parent_connection_id: &str,
        key: &DelegationMatchKey,
        now: Instant,
    ) -> Option<String> {
        let mut map = self.tool_calls.inner.lock().await;
        let bucket = map.get_mut(parent_connection_id)?;

        // Evict every stale UNKEYED entry up front. The key-match scan below
        // ignores unkeyed entries anyway (they carry no key to match), but
        // GC'ing here keeps the queue bounded during the poll loop and
        // consistent with `take_pending_tool_call_at`'s view, so the
        // post-budget last resort never hands out an aged-out id. Mirrors that
        // TTL skip but covers entries at any position (not just the front).
        bucket.pending.retain(|p| {
            // Keyed entries are NEVER aged out. Each identifies one specific
            // `delegate_to_agent` invocation and is claimable ONLY by an exact
            // key match (never by FIFO — see below), so it cannot mis-bind a
            // different delegation no matter how old it gets. And it MUST
            // survive until its MCP round-trip arrives, which the host may
            // serialize arbitrarily far behind earlier long-running
            // delegations: Claude Code runs parallel `delegate_to_agent` calls
            // SEQUENTIALLY, so the 2nd call's round-trip only fires after the
            // 1st child finishes. Observed in the wild — a 2nd delegation whose
            // tool_call registered, then waited 77s (past the old 60s TTL) for
            // its round-trip while the 1st ran; age-evicting it here orphaned
            // it to a synthetic id and left the parent card stuck on
            // "sub-agent running…". Only UNKEYED (anonymous, arrival-order
            // correlated) entries keep the age-based GC, since a stale one
            // could be mis-claimed via the FIFO path. Keyed memory stays bounded
            // by exact-match claim, terminal tombstoning, and
            // `drop_pending_tool_calls_for_parent` on connection teardown — not
            // by this TTL.
            if p.match_key.is_some() {
                return true;
            }
            let fresh = now.duration_since(p.registered_at) <= PENDING_TOOL_CALL_TTL;
            if !fresh {
                let age_secs = now.duration_since(p.registered_at).as_secs();
                tracing::info!(
                    "[delegation] evicting stale UNKEYED ACP tool_call_id={} (age={age_secs}s) on conn={parent_connection_id}",
                    p.tool_call_id
                );
            }
            fresh
        });

        let claimed = if let Some(pos) = bucket
            .pending
            .iter()
            .position(|p| p.match_key.as_ref() == Some(key))
        {
            // Exact (agent_type, task) match: deterministic correlation
            // regardless of ACP-vs-MCP arrival order or how many delegations
            // are in flight.
            bucket.pending.remove(pos).map(|p| p.tool_call_id)
        } else {
            // No exact key match. We deliberately do NOT claim an unkeyed entry
            // here — not even the oldest, not even the only one. An unkeyed
            // pending entry may belong to a DIFFERENT parallel delegation whose
            // own round-trip hasn't yet registered (or keyed) its `tool_call`,
            // and claiming it by arrival order would steal that sibling's id —
            // the mis-bind this machinery exists to prevent.
            //
            // Crucially, the ENTRY's age is the wrong clock for "no key is
            // coming": a host can serialize a round-trip arbitrarily far behind
            // its `tool_call` registration (see the retain block / the
            // `keyed_entry_survives_past_ttl` case), so even an old lone unkeyed
            // entry can still be a sibling's. The CALLER's own wait is the right
            // clock. So return `None` and let
            // `claim_pending_tool_call_with_brief_wait` poll: if this
            // delegation's key lands (initial register or a later backfill) we
            // bind by the exact match above; only after the caller has spent the
            // FULL budget does its post-budget last resort
            // (`take_pending_tool_call`) claim the oldest unkeyed id in arrival
            // order — the best a genuinely keyless host allows, and the point at
            // which waiting longer cannot improve correlation.
            None
        };

        if let Some(id) = &claimed {
            bucket.consumed.push_back((id.clone(), now));
        }
        if bucket.pending.is_empty()
            && bucket.consumed.is_empty()
            && bucket.late_bindings.is_empty()
        {
            map.remove(parent_connection_id);
        }
        claimed
    }

    /// Consume an explicit `parent_tool_use_id` that the MCP client supplied
    /// directly via `_meta.tool_use_id` (the precise-binding path; most clients
    /// omit it). In that case `handle_request` does NOT run the claim path, so
    /// the matching pending entry the lifecycle dispatcher registered off the
    /// parent's ACP stream would otherwise never be consumed — and because
    /// keyed entries are now retained indefinitely, it would linger and could
    /// be mis-claimed by a *later* delegation sharing the same
    /// `(agent_type, task, working_dir)` key, retargeting that delegation's
    /// writes/events at the wrong (already-handled) card.
    ///
    /// Remove the entry from the pending queue AND record the id as consumed.
    /// Recording consumed also covers the MCP-before-ACP race: a later ACP
    /// registration for the same id is dropped by the Tier-1 consumed check in
    /// `register_pending_tool_call_with_key_at`, so the entry can't reappear
    /// regardless of arrival order.
    async fn consume_explicit_tool_call(&self, parent_connection_id: &str, tool_call_id: &str) {
        let mut map = self.tool_calls.inner.lock().await;
        let bucket = map.entry(parent_connection_id.to_string()).or_default();
        bucket.pending.retain(|p| p.tool_call_id != tool_call_id);
        if !bucket.consumed.iter().any(|(id, _)| id == tool_call_id) {
            bucket
                .consumed
                .push_back((tool_call_id.to_string(), Instant::now()));
        }
    }

    /// Resolve a TERMINAL parent `tool_call_id` under one tracker lock. An exact
    /// `(parent_connection_id, task_id)` match claims the late binding whose
    /// `call_id` equals that task id and records the real ACP id as consumed.
    /// Without that identity, the method can only tombstone an already-pending
    /// real id; it never selects a binding by key, FIFO, or global order.
    ///
    /// The hazard: keyed pending entries are retained regardless of age (see the
    /// retain block in `take_matching_tool_call_at`), so if a `delegate_to_agent`
    /// tool call goes terminal without its MCP round-trip ever reaching the
    /// broker (the call failed, the turn was interrupted, the companion never
    /// dispatched), its entry would linger forever and a LATER delegation sharing
    /// the same `(agent_type, task, working_dir)` key would claim this dead id,
    /// retargeting its writes/events at the wrong card. Same hazard
    /// `consume_explicit_tool_call` guards on the explicit-id path; this is its
    /// terminal-status sibling.
    ///
    /// Safe synchronously, no grace window: a terminal `completed` can only
    /// arrive AFTER the round-trip's claim already removed the entry (the ack
    /// that claim produces is what lets the parent's tool call return), and a
    /// serialized sibling still awaiting its (observed 77s-late) round-trip is
    /// NON-terminal while it waits — so this never evicts a live entry.
    ///
    /// Duplicate terminal events see the consumed id and no-op. A wrong parent,
    /// canceled/expired binding, or unknown task id cannot cross-claim another
    /// parent's binding. Returns whether a pending id was tombstoned; exact late
    /// binding has its own completion log in `apply_late_tool_call_binding`.
    pub async fn resolve_terminal_tool_call_by_task_id(
        &self,
        parent_connection_id: &str,
        tool_call_id: &str,
        task_id: Option<&str>,
    ) -> bool {
        let resolution = {
            let mut map = self.tool_calls.inner.lock().await;
            let Some(bucket) = map.get_mut(parent_connection_id) else {
                return false;
            };
            bucket.resolve_terminal_tool_call(tool_call_id, task_id, Instant::now())
        };
        match resolution {
            TerminalToolCallResolution::Ignored => false,
            TerminalToolCallResolution::Tombstoned => true,
            TerminalToolCallResolution::LateBound(binding) => {
                self.apply_late_tool_call_binding(binding, tool_call_id.to_string())
                    .await;
                false
            }
        }
    }

    /// Backward-compatible unkeyed terminal tombstone path.
    pub async fn tombstone_pending_tool_call(
        &self,
        parent_connection_id: &str,
        tool_call_id: &str,
    ) -> bool {
        self.resolve_terminal_tool_call_by_task_id(parent_connection_id, tool_call_id, None)
            .await
    }

    /// Correlate an MCP `delegate_to_agent` round-trip to the parent's
    /// real ACP `tool_call_id`, polling briefly to absorb the race between
    /// two independent arrival paths for the same invocation:
    ///
    ///   * ACP `session/update(tool_call)` → in-process bus → lifecycle
    ///     dispatcher → `register_pending_tool_call_with_key`
    ///   * MCP `tools/call` → stdio round-trip → companion → `handle_request`
    ///
    /// Correlation is by the `(agent_type, task, working_dir)` key (carried in
    /// both the ACP `raw_input` and the MCP call), so several `delegate_to_agent`
    /// calls firing in parallel each bind to their own `tool_call_id`
    /// regardless of arrival order — pure FIFO mis-assigned them (swapping
    /// the child shown under each card) or, when one MCP round-trip out-raced
    /// its ACP event, orphaned the loser to a synthetic `delegation-<uuid>`
    /// (the parent UI then never paints "view session" and the card hangs on
    /// "sub-agent running…", because the frontend keys its binding map by
    /// the agent's real `tool_call_id`).
    ///
    /// As a last resort after the budget — and the ONLY place arrival-order
    /// FIFO is applied — claim the oldest unkeyed id, so a sibling whose
    /// registration was unusually delayed, or a genuinely keyless host, still
    /// yields a *real* id rather than a synthetic one. Deferring FIFO until the
    /// full budget has elapsed is what makes it safe: in-loop we bind ONLY by
    /// exact key match, so a round-trip can't FIFO-steal a sibling's
    /// not-yet-keyed id while that sibling's own registration is still in
    /// flight (the entry's age is no proof a key won't still arrive). A
    /// synthetic id only results when no unkeyed id is claimable for the whole
    /// budget — only keyed siblings remain, or the queue stays genuinely empty.
    async fn claim_pending_tool_call_with_brief_wait(
        &self,
        parent_connection_id: &str,
        key: &DelegationMatchKey,
    ) -> Option<String> {
        if let Some(id) = self
            .take_matching_tool_call(parent_connection_id, key)
            .await
        {
            return Some(id);
        }
        for _ in 0..CLAIM_POLL_ATTEMPTS {
            tokio::time::sleep(CLAIM_POLL_INTERVAL).await;
            if let Some(id) = self
                .take_matching_tool_call(parent_connection_id, key)
                .await
            {
                return Some(id);
            }
        }
        // Budget exhausted with no key match. As a last resort claim the
        // oldest UNKEYED pending id (a host that shipped no parseable
        // `raw_input`, or a mixed-shape race) — a real id beats a synthetic
        // placeholder that orphans the parent UI binding. Crucially this
        // never claims a KEYED entry: those belong to specific in-flight
        // delegations and are reserved for their own exact-key-match
        // round-trip, so when only keyed siblings remain the caller falls
        // through to a synthetic id rather than stealing a sibling's binding
        // (which would just move the dead card from one delegation to another).
        self.take_pending_tool_call(parent_connection_id).await
    }

    async fn cancel_late_tool_call_binding(&self, parent_connection_id: &str, call_id: &str) {
        let removed = {
            let mut map = self.tool_calls.inner.lock().await;
            let Some(bucket) = map.get_mut(parent_connection_id) else {
                return;
            };
            let removed = bucket.cancel_late_binding(call_id, Instant::now());
            if bucket.pending.is_empty()
                && bucket.consumed.is_empty()
                && bucket.late_bindings.is_empty()
            {
                map.remove(parent_connection_id);
            }
            removed
        };
        if removed {
            self.pending
                .inner
                .lock()
                .await
                .clear_late_binding_state(call_id);
            tracing::info!(
                task_id = call_id,
                parent_connection_id,
                "[delegation] removed canceled late tool-call binding"
            );
        }
    }

    async fn apply_late_tool_call_binding(
        &self,
        binding: LateToolCallBinding,
        tool_call_id: String,
    ) {
        let _gate = binding.gate.lock().await;
        self.persist_late_tool_call_binding(&binding, &tool_call_id)
            .await;
        self.publish_late_binding_started(&binding, &tool_call_id)
            .await;
        let projection = self
            .record_late_tool_call_binding(&binding.call_id, &tool_call_id)
            .await;
        if let Some(projection) = projection {
            self.publish_late_binding_completed(&binding, &tool_call_id, projection)
                .await;
        }
        tracing::info!(
            task_id = %binding.call_id,
            parent_connection_id = %binding.parent_connection_id,
            tool_call_id = %tool_call_id,
            "[delegation] synthetic parent id rebound to ACP tool_call_id"
        );
    }

    async fn persist_late_tool_call_binding(
        &self,
        binding: &LateToolCallBinding,
        tool_call_id: &str,
    ) {
        let result = self
            .spawner
            .bind_parent_tool_call(
                binding.child_conversation_id,
                binding.parent_conversation_id,
                &binding.call_id,
                tool_call_id,
            )
            .await;
        if let Err(error) = result {
            tracing::error!(
                task_id = %binding.call_id,
                child_conversation_id = binding.child_conversation_id,
                tool_call_id,
                error = %error,
                "[delegation] failed to persist late tool-call binding"
            );
        }
    }

    async fn record_late_tool_call_binding(
        &self,
        call_id: &str,
        tool_call_id: &str,
    ) -> Option<LateCompletionProjection> {
        let mut inner = self.pending.inner.lock().await;
        if let Some(projection) = inner.late_binding_terminals.remove(call_id) {
            inner.late_binding_pins.remove(call_id);
            return Some(projection);
        }
        if let Some(task) = inner.running.get_mut(call_id) {
            task.parent_tool_use_id = tool_call_id.to_string();
            inner.late_binding_pins.remove(call_id);
            return None;
        }
        let projection = inner
            .completed
            .get(call_id)
            .and_then(late_completion_projection);
        inner.clear_late_binding_state(call_id);
        projection
    }

    async fn publish_late_binding_started(
        &self,
        binding: &LateToolCallBinding,
        tool_call_id: &str,
    ) {
        self.write_meta_if_real(
            &binding.parent_connection_id,
            tool_call_id,
            build_delegation_meta(
                "running",
                Some(&binding.child_connection_id),
                Some(binding.child_conversation_id),
                None,
                None,
                None,
            ),
        )
        .await;
        self.emit_started_if_real(
            &binding.parent_connection_id,
            tool_call_id,
            &binding.child_connection_id,
            binding.child_conversation_id,
            binding.agent_type,
        )
        .await;
    }

    async fn publish_late_binding_completed(
        &self,
        binding: &LateToolCallBinding,
        tool_call_id: &str,
        projection: LateCompletionProjection,
    ) {
        self.write_meta_if_real(
            &binding.parent_connection_id,
            tool_call_id,
            build_delegation_meta(
                projection.status,
                Some(&binding.child_connection_id),
                Some(binding.child_conversation_id),
                projection.error_code.as_deref(),
                projection.preview.as_deref(),
                Some(projection.duration_ms),
            ),
        )
        .await;
        self.emit_completed_if_real(
            &binding.parent_connection_id,
            tool_call_id,
            &binding.child_connection_id,
            binding.child_conversation_id,
            binding.agent_type,
            projection.result,
        )
        .await;
    }

    /// Remove `(parent, handle)` from the pre-cancel set, returning whether it was
    /// present. Used by `handle_request` at two checkpoints (entry + just
    /// after pending registration) so a cancel that lost the race with the
    /// MCP round-trip still wins. The set is single-shot per handle —
    /// taking it here means a subsequent `cancel_by_external_handle` will
    /// have to find the pending entry on its own.
    async fn take_pre_canceled_handle(&self, parent_connection_id: &str, handle: &str) -> bool {
        let key = (parent_connection_id.to_string(), handle.to_string());
        let mut state = self.pre_canceled_handles.inner.lock().await;
        if state.set.remove(&key) {
            // Best-effort companion-side cleanup of `order` so a later
            // FIFO eviction doesn't burn a slot. Linear scan is fine —
            // PRE_CANCELED_CAP is small.
            if let Some(pos) = state.order.iter().position(|candidate| candidate == &key) {
                state.order.remove(pos);
            }
            true
        } else {
            false
        }
    }

    /// Insert `(parent, handle)` into the pre-cancel set with FIFO eviction at
    /// [`PRE_CANCELED_CAP`]. Idempotent — re-inserting an existing handle
    /// is a no-op.
    async fn buffer_pre_canceled_handle(&self, parent_connection_id: &str, handle: String) {
        let key = (parent_connection_id.to_string(), handle);
        let mut state = self.pre_canceled_handles.inner.lock().await;
        if !state.set.insert(key.clone()) {
            return;
        }
        state.order.push_back(key);
        while state.order.len() > PRE_CANCELED_CAP {
            if let Some(evicted) = state.order.pop_front() {
                state.set.remove(&evicted);
            }
        }
    }

    /// Forget every pending and recently-consumed tool_call id for the
    /// given parent. Called when the parent connection tears down so
    /// stale ids don't bind to a future reuse of the same connection_id
    /// (UUIDs make that unlikely but cheap to defend against), and so a
    /// fresh connection on the reused id is not blocked by the
    /// consumed memory of the previous one.
    pub async fn drop_pending_tool_calls_for_parent(&self, parent_connection_id: &str) {
        self.drop_tool_calls_for_parent(parent_connection_id, false)
            .await;
        self.drop_pre_canceled_handles_for_parent(parent_connection_id)
            .await;
    }

    async fn drop_pre_canceled_handles_for_parent(&self, parent_connection_id: &str) {
        let mut state = self.pre_canceled_handles.inner.lock().await;
        state
            .set
            .retain(|(parent, _)| parent != parent_connection_id);
        state
            .order
            .retain(|(parent, _)| parent != parent_connection_id);
    }

    /// Core of the tool_call-tracker drop, shared by the two cancel scopes.
    ///
    /// * `keep_consumed == false` — genuine connection teardown: remove the
    ///   whole bucket (`pending` + `consumed`). The connection is going away,
    ///   so nothing it remembered can mis-bind a future delegation, and a
    ///   reused connection_id must start clean.
    /// * `keep_consumed == true` — turn/prompt cancel with the parent
    ///   connection STILL ALIVE: TOMBSTONE the cancelled turn's unclaimed
    ///   `pending` ids into `consumed` and RETAIN the existing `consumed`. Both
    ///   the already-claimed ids AND the just-cancelled turn's unclaimed ids
    ///   must keep rejecting a host re-emit (e.g. a terminal status-flip): the
    ///   Tier-1 consumed check in `register_pending_tool_call_with_key_at` drops
    ///   the re-emit, so a stale id can't re-register as fresh `pending` and
    ///   mis-bind the next same-key delegation on this live connection. Merely
    ///   CLEARING the unclaimed ids would leave them re-registerable, reopening
    ///   that hole for the unclaimed half (the claimed half was already safe via
    ///   `consumed`). Retention is connection-scoped and released on teardown —
    ///   the same unbounded-but-bounded-by-delegation-count envelope `consumed`
    ///   already lives in for normal end_turn delegations (see
    ///   [`ToolCallTrackerBucket`]).
    ///
    /// Tombstoning ALL of `pending` here is safe (no turn/generation tag
    /// needed): `run_conversation_loop` drives at most ONE `session/prompt`
    /// future per connection at a time (see `acp/connection.rs`), and a
    /// parent-side `tool_call` only streams while its prompt future is in
    /// flight, so every `pending` id belongs to the single active turn — the one
    /// being cancelled — or is a stale leftover from an earlier turn that should
    /// be tombstoned regardless. (The per-connection `prompt_lock` only
    /// serializes the prompt-SEND handshake, not the turn, so it is NOT the
    /// source of this invariant.) The cancelled turn's serialized MCP round-trip
    /// won't arrive after cancel, so nothing legitimate is lost.
    async fn drop_tool_calls_for_parent(&self, parent_connection_id: &str, keep_consumed: bool) {
        let mut map = self.tool_calls.inner.lock().await;
        let mut inner = self.pending.inner.lock().await;
        let call_ids =
            drop_tool_calls_for_parent_locked(&mut map, parent_connection_id, keep_consumed);
        for call_id in call_ids {
            inner.clear_late_binding_state(&call_id);
        }
    }

    pub async fn set_config(&self, cfg: DelegationConfig) {
        let cap_bytes = cfg.completed_cache_cap_bytes;
        *self.config.lock().await = cfg;
        // Seed the byte cap into the pending-calls bucket so `insert_completed`
        // reads it lock-free (it already holds the pending lock). Acquired AFTER
        // the config guard above is dropped — sequential, never nested — so no
        // path locks `config` under `pending` or vice-versa (deadlock-free).
        // Then prune existing per-parent caches: a LOWERED cap must free memory
        // now, not lazily on each parent's next completion (which may never
        // arrive for an idle parent).
        let mut inner = self.pending.inner.lock().await;
        inner.completed_cap_bytes = cap_bytes;
        inner.enforce_completed_cap_all_parents();
    }

    pub async fn set_concurrency_limit(&self, limit: u32) {
        *self.concurrency_limit.lock().await =
            crate::commands::agent_concurrency::clamp_limit(limit);
    }

    async fn concurrency_semaphore(&self, root_conversation_id: i32) -> Arc<Semaphore> {
        let mut pools = self.concurrency_pools.lock().await;
        let limit = *self.concurrency_limit.lock().await as usize;
        pools
            .entry(root_conversation_id)
            .or_insert_with(|| Arc::new(Semaphore::new(limit)))
            .clone()
    }

    async fn root_conversation_id(&self, conversation_id: i32) -> Result<i32, DelegationError> {
        let mut current = conversation_id;
        for _ in 0..64 {
            match self.depth_lookup.parent_of(current).await? {
                Some(parent) if parent != current => current = parent,
                _ => return Ok(current),
            }
        }
        Err(DelegationError::SubagentRuntimeError(
            "conversation ancestry exceeded safety limit".into(),
        ))
    }

    async fn acquire_concurrency_permit(
        &self,
        wait: ConcurrencyWait<'_>,
    ) -> Result<Arc<ConcurrencyPermit>, ()> {
        let semaphore = self.concurrency_semaphore(wait.root_conversation_id).await;
        if semaphore.available_permits() == 0 {
            tracing::debug!(
                root_conversation_id = wait.root_conversation_id,
                "waiting for Agent concurrency permit"
            );
        }
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            let permit = tokio::select! {
                permit = semaphore.clone().acquire_owned() => {
                    Some(permit.map_err(|_| ())?)
                }
                _ = interval.tick() => None,
            };
            if self.concurrency_wait_canceled(&wait).await {
                tracing::debug!(
                    root_conversation_id = wait.root_conversation_id,
                    "canceled while waiting for Agent concurrency permit"
                );
                return Err(());
            }
            if let Some(permit) = permit {
                tracing::debug!(
                    root_conversation_id = wait.root_conversation_id,
                    "acquired Agent concurrency permit"
                );
                return Ok(Arc::new(ConcurrencyPermit {
                    _permit: permit,
                    root_conversation_id: wait.root_conversation_id,
                }));
            }
        }
    }

    async fn concurrency_wait_canceled(&self, wait: &ConcurrencyWait<'_>) -> bool {
        if self.take_inflight_cancel(wait.inflight_id).await {
            return true;
        }
        let Some(handle) = wait.external_handle else {
            return false;
        };
        if !self
            .take_pre_canceled_handle(wait.parent_connection_id, handle)
            .await
        {
            return false;
        }
        self.drop_inflight(wait.inflight_id).await;
        true
    }

    async fn clear_canceled_tool_call_binding(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        late_match_key: Option<&DelegationMatchKey>,
    ) {
        if let Some(match_key) = late_match_key {
            let _ = self
                .take_matching_tool_call(parent_connection_id, match_key)
                .await;
        } else if !is_synthetic_parent_tool_use_id(parent_tool_use_id) {
            self.consume_explicit_tool_call(parent_connection_id, parent_tool_use_id)
                .await;
        }
    }

    pub async fn config_snapshot(&self) -> DelegationConfig {
        self.config.lock().await.clone()
    }

    /// If this in-flight setup has been flagged canceled by a parent cancel,
    /// deregister it and return true. One lock acquisition; used at the
    /// pre-spawn / post-spawn checkpoints in `handle_request`.
    async fn take_inflight_cancel(&self, inflight_id: u64) -> bool {
        let mut inner = self.pending.inner.lock().await;
        if inner.inflight_canceled(inflight_id) {
            inner.deregister_inflight(inflight_id);
            true
        } else {
            false
        }
    }

    /// Drop this setup's in-flight record. Called on each `handle_request`
    /// early-return that isn't a park hand-off (the park region deregisters
    /// inline, atomically with `calls.insert`).
    async fn drop_inflight(&self, inflight_id: u64) {
        self.pending
            .inner
            .lock()
            .await
            .deregister_inflight(inflight_id);
    }

    /// Async entry point for `delegate_to_agent`. Does the bounded setup
    /// (claim/depth checks → spawn → send first prompt), registers the task in
    /// `running`, and returns a `Running` ack [`DelegationTaskReport`] WITHOUT
    /// waiting for the child to finish. The child resolves later via the
    /// lifecycle → [`complete_call`] (or a cancel path), which migrates the task
    /// into `completed` and wakes any `get_delegation_status` long-poll.
    ///
    /// Returns a terminal report instead of a `Running` ack in three cases: the
    /// child finished during setup (fast/empty turn), a parent cancel reached it
    /// mid-setup, or setup itself failed (disabled / depth / spawn / send).
    ///
    /// All the setup-window race machinery (`setups` / `early_*` / `inflight`)
    /// is unchanged — it governs terminals that beat registration, which is
    /// orthogonal to whether the caller then blocks. The only change vs. the old
    /// `handle_request` is that "park a `oneshot` and await it" becomes "insert a
    /// [`RunningTask`] and return the ack."
    #[tracing::instrument(
        name = "delegation_task",
        skip_all,
        fields(
            parent_connection_id = %req.parent_connection_id,
            parent_tool_use_id = %req.parent_tool_use_id,
            agent_type = ?req.agent_type,
            working_dir = ?req.working_dir,
            child_connection_id = tracing::field::Empty,
            task_id = tracing::field::Empty,
        )
    )]
    pub async fn start_delegation(&self, mut req: DelegationRequest) -> DelegationTaskReport {
        if !crate::acp::registry::is_executable_identity(req.agent_type) {
            tracing::warn!(
                agent_type = %req.agent_type,
                "[delegation] rejected untrusted Agent identity before launch"
            );
            return report_err(req.agent_type, DelegationError::InvalidAgentType, None);
        }
        // Register this setup as the VERY FIRST thing — before the pre-cancel
        // check's `.await` and the (possibly multi-second) claim poll — so a
        // parent cancel landing ANYWHERE from here to park reaches it, not just
        // after park (which is all the `cancel_by_parent*` parked-call drain
        // covers on its own). The only residual gap is a cancel firing before
        // the broker is even invoked for this request, which no
        // in-`handle_request` mechanism can observe. Deregistered on every exit
        // path below: each early-return via `drop_inflight` /
        // `take_inflight_cancel`, or inline at park (atomically with
        // `calls.insert`).
        let inflight_id = self
            .pending
            .inner
            .lock()
            .await
            .register_inflight(&req.parent_connection_id);
        // Pre-cancel short-circuit. If the MCP companion already received
        // `notifications/cancelled` for this `tools/call` before we even
        // started processing (cancel ran ahead of the UDS round-trip), we
        // claim the handle from the pre-cancel set and bail without
        // spawning anything — the caller will not be receiving our
        // response either way (the companion suppresses it per MCP spec).
        if let Some(handle) = req.external_handle.as_deref() {
            if self
                .take_pre_canceled_handle(&req.parent_connection_id, handle)
                .await
            {
                self.drop_inflight(inflight_id).await;
                // Bailing here BEFORE the claim path means this delegation never
                // consumes the ACP `tool_call_id` the lifecycle keyed for it. As
                // keyed entries are retained indefinitely, a leftover would let a
                // *later* same-`(agent_type, task, working_dir)` delegation claim
                // this canceled call's id and bind its writes/events to the wrong
                // card. Drain it now (idempotent; the turn-end tombstone is the
                // backstop if the ACP event hasn't registered yet).
                if req.parent_tool_use_id.is_empty() {
                    let key = DelegationMatchKey {
                        agent_type: req.agent_type,
                        task: req.task.clone(),
                        working_dir: req.requested_working_dir.clone(),
                    };
                    let _ = self
                        .take_matching_tool_call(&req.parent_connection_id, &key)
                        .await;
                } else {
                    self.consume_explicit_tool_call(
                        &req.parent_connection_id,
                        &req.parent_tool_use_id,
                    )
                    .await;
                }
                return report_err(
                    req.agent_type,
                    DelegationError::Canceled {
                        reason: "canceled before spawn".into(),
                    },
                    None,
                );
            }
        }
        // MCP clients usually don't populate `_meta.tool_use_id`, so the
        // listener will pass through an empty string. Claim the matching
        // ACP-side `tool_call_id` for this parent by task text — with a brief
        // poll loop so an MCP round-trip that out-races the in-process ACP
        // `session/update` doesn't fall back to a synthetic id (which breaks
        // the parent UI's `parent_tool_use_id` binding). Falls back to a UUID
        // placeholder only when no id arrives within the wait budget.
        let mut late_match_key = None;
        if req.parent_tool_use_id.is_empty() {
            let match_key = DelegationMatchKey {
                agent_type: req.agent_type,
                task: req.task.clone(),
                working_dir: req.requested_working_dir.clone(),
            };
            req.parent_tool_use_id = match self
                .claim_pending_tool_call_with_brief_wait(&req.parent_connection_id, &match_key)
                .await
            {
                Some(tool_call_id) => tool_call_id,
                None => {
                    tracing::warn!(
                        "[delegation] ACP tool_call_id not yet available on conn={}; arming late binding",
                        req.parent_connection_id
                    );
                    late_match_key = Some(match_key);
                    format!("delegation-{}", uuid::Uuid::new_v4())
                }
            };
        } else {
            // The client gave us the real ACP tool_call_id directly
            // (`_meta.tool_use_id`), so we skip the claim path — but the
            // lifecycle dispatcher may already have registered that same id as
            // a (now indefinitely-retained) keyed pending entry. Consume it so
            // it can't linger and be mis-claimed by a later same-key
            // delegation. Idempotent and order-independent (see the method).
            self.consume_explicit_tool_call(&req.parent_connection_id, &req.parent_tool_use_id)
                .await;
        }
        let cfg = self.config_snapshot().await;
        if !cfg.enabled {
            self.drop_inflight(inflight_id).await;
            return report_err(
                req.agent_type,
                DelegationError::Canceled {
                    reason: "delegation disabled".into(),
                },
                None,
            );
        }

        // --- Depth pre-check ----------------------------------------------------
        // We walk up to `limit + 1` so we know whether the *new* child would
        // sit at >= limit. Cycles/dead chains saturate at the cap.
        let lookup = self.depth_lookup.clone();
        let parent_depth = match crate::acp::delegation::depth::compute_depth(
            req.parent_conversation_id,
            |id| {
                let lookup = lookup.clone();
                async move { lookup.parent_of(id).await }
            },
            cfg.depth_limit + 1,
        )
        .await
        {
            Ok(d) => d,
            Err(e) => {
                self.drop_inflight(inflight_id).await;
                return report_err(req.agent_type, e, None);
            }
        };
        // The child the broker is about to create would sit at `parent_depth + 1`.
        // Reject only when the *child* depth would strictly exceed the limit;
        // a child sitting exactly at `depth_limit` is allowed.
        if parent_depth + 1 > cfg.depth_limit {
            self.drop_inflight(inflight_id).await;
            return report_err(
                req.agent_type,
                DelegationError::DepthLimitExceeded {
                    current_depth: parent_depth,
                    limit: cfg.depth_limit,
                },
                None,
            );
        }

        // Resolve the ancestry before creating the child so nested delegation
        // shares the root session's semaphore rather than getting a new pool.
        let root_conversation_id = match self.root_conversation_id(req.parent_conversation_id).await
        {
            Ok(root) => root,
            Err(error) => {
                self.drop_inflight(inflight_id).await;
                return report_err(req.agent_type, error, None);
            }
        };
        let concurrency_permit = match self
            .acquire_concurrency_permit(ConcurrencyWait {
                root_conversation_id,
                inflight_id,
                parent_connection_id: &req.parent_connection_id,
                external_handle: req.external_handle.as_deref(),
            })
            .await
        {
            Ok(permit) => permit,
            Err(()) => {
                self.clear_canceled_tool_call_binding(
                    &req.parent_connection_id,
                    &req.parent_tool_use_id,
                    late_match_key.as_ref(),
                )
                .await;
                return report_err(
                    req.agent_type,
                    DelegationError::Canceled {
                        reason: "parent canceled while waiting for concurrency slot".into(),
                    },
                    None,
                );
            }
        };

        // --- Spawn child connection --------------------------------------------
        // Pull per-agent overrides from the broker config (defaults to empty).
        // Cloning is cheap — `AgentDelegationDefaults` is at most one Option<String>
        // and a small BTreeMap, and the spawner consumes both fields by value.
        let (explicit_mode_id, preferred_config_values) = cfg
            .agent_defaults
            .get(&req.agent_type)
            .map(|d: &AgentDelegationDefaults| (d.mode_id.clone(), d.config_values.clone()))
            .unwrap_or((None, BTreeMap::new()));
        let mode_source = if explicit_mode_id.is_some() {
            "user"
        } else {
            "product_default"
        };
        let preferred_mode_id =
            explicit_mode_id.or_else(|| Some(automatic_mode_id(req.agent_type).to_string()));
        tracing::info!(
            parent_conversation_id = req.parent_conversation_id,
            mode_id = preferred_mode_id.as_deref().unwrap_or_default(),
            mode_source,
            config_override_count = preferred_config_values.len(),
            "[delegation] resolved child session defaults"
        );
        // Checkpoint #1 (opportunistic): if a parent cancel already landed
        // during the claim/depth phase, bail before spawning a child the parent
        // has abandoned. No child exists yet, so there's nothing to tear down.
        if self
            .concurrency_wait_canceled(&ConcurrencyWait {
                root_conversation_id,
                inflight_id,
                parent_connection_id: &req.parent_connection_id,
                external_handle: req.external_handle.as_deref(),
            })
            .await
        {
            self.clear_canceled_tool_call_binding(
                &req.parent_connection_id,
                &req.parent_tool_use_id,
                late_match_key.as_ref(),
            )
            .await;
            return report_err(
                req.agent_type,
                DelegationError::Canceled {
                    reason: "parent canceled".into(),
                },
                None,
            );
        }
        let child_connection_id = match self
            .spawner
            .spawn(
                &req.parent_connection_id,
                req.agent_type,
                req.working_dir.clone(),
                preferred_mode_id,
                preferred_config_values,
            )
            .await
        {
            Ok(id) => id,
            Err(e) => {
                self.drop_inflight(inflight_id).await;
                return report_err(
                    req.agent_type,
                    DelegationError::SpawnFailed(e.to_string()),
                    None,
                );
            }
        };

        // Checkpoint #2: a parent cancel that landed during spawn() — the child
        // now exists but no prompt has been sent, so disconnect it (mirroring
        // the send-failure path's disconnect-only teardown) and bail. This is
        // the primary guard for the spawn window, which can block while the
        // agent process starts up.
        if self
            .concurrency_wait_canceled(&ConcurrencyWait {
                root_conversation_id,
                inflight_id,
                parent_connection_id: &req.parent_connection_id,
                external_handle: req.external_handle.as_deref(),
            })
            .await
        {
            let _ = self.spawner.disconnect(&child_connection_id).await;
            self.clear_canceled_tool_call_binding(
                &req.parent_connection_id,
                &req.parent_tool_use_id,
                late_match_key.as_ref(),
            )
            .await;
            return report_err(
                req.agent_type,
                DelegationError::Canceled {
                    reason: "parent canceled".into(),
                },
                None,
            );
        }

        // --- Send linked prompt ------------------------------------------------
        let call_id = uuid::Uuid::new_v4().to_string();
        // Now that the child connection and task id exist, fill the span's empty
        // fields so every subsequent log line in this delegation carries the
        // parent→child linkage (see the `delegation_task` span on this fn).
        tracing::Span::current().record("child_connection_id", child_connection_id.as_str());
        tracing::Span::current().record("task_id", call_id.as_str());
        let link = DelegationLink {
            parent_conversation_id: req.parent_conversation_id,
            parent_tool_use_id: req.parent_tool_use_id.clone(),
            delegation_call_id: call_id.clone(),
        };

        // Reserve this delegation (both ids) BEFORE sending its first prompt.
        // `send_prompt_linked_for_delegation` persists the delegation link onto
        // the child row (arming the lifecycle resolver) AND dispatches the
        // prompt — after which a fast/empty turn's `TurnComplete` OR an
        // immediate child-connection failure can fire before we park the pending
        // entry below. The reservation lets those terminal events buffer their
        // outcome (see `PendingInner`) for the park to drain, rather than
        // no-oping and stranding `rx.await`. There is no `.await` between this
        // reservation and `send_prompt` (so nothing the child does can be
        // observed before the reservation is in place); it's cleared at park or
        // on the send-failure path. Reserving by `call_id` AND
        // `child_connection_id` lets each resolver gate on the id it holds —
        // `complete_call` the `call_id`, `cancel_by_child_connection` the
        // `child_connection_id`.
        self.pending
            .inner
            .lock()
            .await
            .reserve(&call_id, &child_connection_id);

        let child_conversation_id = match self
            .spawner
            .send_prompt_linked_for_delegation(&child_connection_id, req.task.clone(), link)
            .await
        {
            Ok(cid) => cid,
            Err(e) => {
                // Setup failed before parking — release the reservation (and
                // discard any terminal that buffered against this delegation in
                // the window) so nothing lingers or mis-binds a future id, and
                // drop the in-flight record in the same lock acquisition.
                {
                    let mut inner = self.pending.inner.lock().await;
                    inner.unreserve(&call_id, &child_connection_id);
                    inner.deregister_inflight(inflight_id);
                }
                let _ = self.spawner.disconnect(&child_connection_id).await;
                return report_err(
                    req.agent_type,
                    DelegationError::SpawnFailed(e.to_string()),
                    None,
                );
            }
        };

        // The child is now running. Stamp the start so terminal paths can
        // report a real `duration_ms`.
        let started_at = Instant::now();

        // --- Mark the parent's tool call as in-flight -------------------------
        // The frontend's DelegationContext seeds its `parent_tool_use_id`-keyed
        // binding map from this meta on snapshot replay, so a page refresh
        // mid-delegation can reconstruct the child connection / conversation
        // ids without depending on the live `delegation_started` event having
        // been received.
        self.write_meta_if_real(
            &req.parent_connection_id,
            &req.parent_tool_use_id,
            build_delegation_meta(
                "running",
                Some(&child_connection_id),
                Some(child_conversation_id),
                None,
                None,
                // No meaningful elapsed yet — the child just started.
                None,
            ),
        )
        .await;

        // Announce the live delegation on the PARENT's event stream so the
        // frontend `DelegationContext` binds the child inline and attaches its
        // live sub-thread. Symmetric with the terminal `emit_completed_if_real`,
        // and — unlike the removed child-stream emit in `send_prompt_linked` —
        // delivered on a stream the parent is already attached to in web/server
        // mode, carrying the real `parent_connection_id`.
        self.emit_started_if_real(
            &req.parent_connection_id,
            &req.parent_tool_use_id,
            &child_connection_id,
            child_conversation_id,
            req.agent_type,
        )
        .await;

        // --- Register pending, or resolve a terminal that beat us -------------
        // Under a single lock, decide this delegation's fate atomically against
        // everything a concurrent resolver may have recorded while we were
        // setting up:
        //   * a child terminal buffered against the reservation — a
        //     `TurnComplete` via `complete_call` (keyed by `call_id`) OR a child
        //     failure via `cancel_by_child_connection` (keyed by
        //     `child_connection_id`); either can race ahead of this park; or
        //   * a parent cancel that flagged this in-flight setup
        //     (`mark_inflight_canceled_for_parent`, which runs in the SAME lock
        //     acquisition that drains the parked `calls`).
        // Precedence: strict first-terminal-wins by arrival stamp. Both a child
        // terminal and a parent cancel carry the `seq` clock value they were
        // recorded at, so whichever landed FIRST wins — a child that completed
        // before the cancel keeps its result; a cancel that beat the completion
        // discards it (the parent had already abandoned the turn). Ties are
        // impossible: every event draws a distinct stamp under this one lock.
        // Only when NOTHING beat us do we park for a future resolver,
        // deregistering the in-flight record adjacent to `calls.insert` with no
        // `.await` between — so a parent cancel serialized AFTER us finds the
        // entry in `calls` and drains it, while one serialized BEFORE us is seen
        // here via its stamp. When a terminal/cancel DID beat us we deliberately
        // DON'T park: resolving inline (never leaving an entry for a second
        // resolver to grab) rules out a double-finalize.
        enum Disposition {
            ChildTerminal(DelegationOutcome),
            ParentCanceled,
            Running,
        }
        // Near-zero elapsed for these setup-window races, but measured for
        // consistency with the normal terminal paths.
        let setup_duration_ms = started_at.elapsed().as_millis() as u64;
        let mut late_binding = late_match_key.map(|match_key| LateToolCallBinding {
            call_id: call_id.clone(),
            match_key,
            parent_connection_id: req.parent_connection_id.clone(),
            parent_conversation_id: req.parent_conversation_id,
            child_connection_id: child_connection_id.clone(),
            child_conversation_id,
            agent_type: req.agent_type,
            gate: Arc::new(Mutex::new(())),
        });
        let late_binding_gate = late_binding
            .as_ref()
            .map(|binding| Arc::clone(&binding.gate));
        let (disposition, claimed_late_binding) = {
            // Parent cancellation takes these locks in the same order. The
            // running/completed transition and late-binding arm therefore form
            // one handoff: cancel either clears the binding afterward or wins
            // first and makes this setup observe ParentCanceled.
            let mut tool_calls = self.tool_calls.inner.lock().await;
            let mut inner = self.pending.inner.lock().await;
            // Each buffered child terminal carries (arrival_stamp, outcome).
            let child_terminal: Option<(u64, DelegationOutcome)> =
                if let Some((stamp, outcome)) = inner.take_early_complete(&call_id) {
                    Some((stamp, outcome))
                } else {
                    inner
                        .take_early_cancel(&child_connection_id)
                        .map(|(stamp, reason)| {
                            (
                                stamp,
                                DelegationOutcome::from_err(
                                    DelegationError::Canceled { reason },
                                    Some(child_conversation_id),
                                ),
                            )
                        })
                };
            let parent_canceled_at = inner.inflight_canceled_at(inflight_id);
            inner.unreserve(&call_id, &child_connection_id);
            // For both terminal dispositions we record the completed result
            // INSIDE this lock (atomically with unreserve/deregister) so a
            // concurrent `get_delegation_status` can never observe the task as
            // neither running nor completed. The `Running` arm inserts the live
            // task instead of parking a `oneshot` — the caller returns the ack.
            let record = |inner: &mut PendingInner, outcome: &DelegationOutcome| {
                inner.insert_completed(
                    &call_id,
                    build_completed(
                        &req.parent_connection_id,
                        child_conversation_id,
                        req.agent_type,
                        setup_duration_ms,
                        outcome,
                    ),
                );
            };
            let disposition = match (child_terminal, parent_canceled_at) {
                // Both raced in the setup window: the earlier arrival stamp wins.
                (Some((child_stamp, outcome)), Some(cancel_stamp)) => {
                    if child_stamp < cancel_stamp {
                        Disposition::ChildTerminal(outcome)
                    } else {
                        Disposition::ParentCanceled
                    }
                }
                // Only a child terminal fired.
                (Some((_, outcome)), None) => Disposition::ChildTerminal(outcome),
                // Only a parent cancel fired.
                (None, Some(_)) => Disposition::ParentCanceled,
                // Nothing beat us — register the running task for a future
                // resolver, deregistering the in-flight record adjacent to the
                // insert with no `.await` between (so a parent cancel serialized
                // AFTER us finds it in `running` and drains it).
                (None, None) => Disposition::Running,
            };
            let claimed_late_binding = if matches!(&disposition, Disposition::ParentCanceled) {
                None
            } else {
                late_binding.take().and_then(|binding| {
                    inner.late_binding_pins.insert(binding.call_id.clone());
                    tool_calls
                        .entry(binding.parent_connection_id.clone())
                        .or_default()
                        .arm_late_binding(binding.clone(), Instant::now())
                        .map(|tool_call_id| (binding, tool_call_id))
                })
            };
            match &disposition {
                Disposition::ChildTerminal(outcome) => record(&mut inner, outcome),
                Disposition::ParentCanceled => record(
                    &mut inner,
                    &canceled_outcome(child_conversation_id, "parent canceled"),
                ),
                Disposition::Running => {
                    inner.running.insert(
                        call_id.clone(),
                        RunningTask {
                            child_connection_id: child_connection_id.clone(),
                            child_conversation_id,
                            parent_connection_id: req.parent_connection_id.clone(),
                            parent_tool_use_id: req.parent_tool_use_id.clone(),
                            agent_type: req.agent_type,
                            external_handle: req.external_handle.clone(),
                            started_at,
                            late_binding_gate: late_binding_gate.clone(),
                            _concurrency_permit: concurrency_permit.clone(),
                        },
                    );
                }
            }
            inner.deregister_inflight(inflight_id);
            (disposition, claimed_late_binding)
        };
        if let Some((binding, tool_call_id)) = claimed_late_binding {
            self.apply_late_tool_call_binding(binding, tool_call_id)
                .await;
        }

        match disposition {
            // A child terminal beat registration. Finalize (terminal meta +
            // DelegationCompleted event + child teardown) and return the
            // terminal report directly. The completed entry was recorded under
            // the disposition lock above; wake any long-poll waiter.
            Disposition::ChildTerminal(outcome) => {
                self.finalize_delegation(
                    &req.parent_connection_id,
                    &req.parent_tool_use_id,
                    &child_connection_id,
                    child_conversation_id,
                    req.agent_type,
                    setup_duration_ms,
                    &outcome,
                )
                .await;
                self.result_notify.notify_waiters();
                return report_from_outcome(
                    Some(call_id),
                    Some(req.agent_type),
                    &outcome,
                    Some(setup_duration_ms),
                );
            }
            // A parent cancel reached this delegation mid-setup — after the
            // prompt was sent, before we registered. Tear the child down
            // ourselves (cancel + disconnect, since a turn is in flight) and
            // return a canceled report. The canceled result was recorded above.
            Disposition::ParentCanceled => {
                self.write_meta_if_real(
                    &req.parent_connection_id,
                    &req.parent_tool_use_id,
                    build_delegation_meta(
                        "failed",
                        Some(&child_connection_id),
                        Some(child_conversation_id),
                        Some("canceled"),
                        None,
                        Some(setup_duration_ms),
                    ),
                )
                .await;
                self.emit_completed_if_real(
                    &req.parent_connection_id,
                    &req.parent_tool_use_id,
                    &child_connection_id,
                    child_conversation_id,
                    req.agent_type,
                    DelegationResultSummary::Err {
                        error_code: "canceled".to_string(),
                    },
                )
                .await;
                let _ = self.spawner.cancel(&child_connection_id).await;
                let _ = self.spawner.disconnect(&child_connection_id).await;
                self.result_notify.notify_waiters();
                return report_from_outcome(
                    Some(call_id),
                    Some(req.agent_type),
                    &canceled_outcome(child_conversation_id, "parent canceled"),
                    Some(setup_duration_ms),
                );
            }
            // Registered in `running` — fall through to the second pre-cancel
            // check, then return the ack.
            Disposition::Running => {}
        }

        // Second pre-cancel check: a `notifications/cancelled` may have landed
        // between the entry-side check and the `running` registration above. If
        // so, drain the task ourselves (so a racing `cancel_by_external_handle`
        // doesn't double-finalize), record the canceled result, and return a
        // canceled report instead of the Running ack.
        if let Some(handle) = req.external_handle.as_deref() {
            if self
                .take_pre_canceled_handle(&req.parent_connection_id, handle)
                .await
            {
                // Capture the elapsed ONCE at terminalization (under the lock,
                // when the running task is removed) so the completed-cache, the
                // parent-card meta, and the returned report all report the same
                // duration. `None` when nothing was drained.
                let canceled_task = {
                    let mut inner = self.pending.inner.lock().await;
                    if let Some(task) = inner.running.remove(&call_id) {
                        let outcome =
                            canceled_outcome(child_conversation_id, "canceled before await");
                        let duration_ms = started_at.elapsed().as_millis() as u64;
                        inner.insert_completed(
                            &call_id,
                            build_completed(
                                &req.parent_connection_id,
                                child_conversation_id,
                                req.agent_type,
                                duration_ms,
                                &outcome,
                            ),
                        );
                        Some((duration_ms, task.parent_tool_use_id))
                    } else {
                        None
                    }
                };
                if let Some((duration_ms, parent_tool_use_id)) = canceled_task {
                    self.cancel_late_tool_call_binding(&req.parent_connection_id, &call_id)
                        .await;
                    self.write_meta_if_real(
                        &req.parent_connection_id,
                        &parent_tool_use_id,
                        build_delegation_meta(
                            "failed",
                            Some(&child_connection_id),
                            Some(child_conversation_id),
                            Some("canceled"),
                            None,
                            Some(duration_ms),
                        ),
                    )
                    .await;
                    self.emit_completed_if_real(
                        &req.parent_connection_id,
                        &parent_tool_use_id,
                        &child_connection_id,
                        child_conversation_id,
                        req.agent_type,
                        DelegationResultSummary::Err {
                            error_code: "canceled".to_string(),
                        },
                    )
                    .await;
                    let _ = self.spawner.cancel(&child_connection_id).await;
                    let _ = self.spawner.disconnect(&child_connection_id).await;
                    self.result_notify.notify_waiters();
                    return report_from_outcome(
                        Some(call_id),
                        Some(req.agent_type),
                        &canceled_outcome(child_conversation_id, "canceled before await"),
                        Some(duration_ms),
                    );
                }
            }
        }

        // Registered and running in the background — return the ack. The child
        // resolves later via the lifecycle → `complete_call` (or a cancel path).
        running_ack(call_id, child_conversation_id, req.agent_type)
    }

    /// Called by the child-session lifecycle subscriber on `TurnComplete`
    /// (success path) or by error mappers (failure path).
    ///
    /// Migrates the task from `running` into `completed` (atomically, under one
    /// lock) and then finalizes (terminal meta + `DelegationCompleted` event +
    /// child teardown) and wakes any `get_delegation_status` long-poll.
    ///
    /// If no entry is in `running` under `call_id`, the outcome is buffered for
    /// a racing `start_delegation` to drain at registration — but ONLY while the
    /// delegation is still reserved (mid-setup). This closes the window where a
    /// fast/empty turn's `TurnComplete` propagates through the lifecycle while
    /// `start_delegation` is still between `send_prompt` and the `running`
    /// insert: the prompt is only *enqueued* by `send_prompt`, and the child
    /// loop emits `TurnComplete` independently, so a completion CAN beat it. When
    /// the `call_id` is no longer reserved the call was already resolved by
    /// another terminal path, so the buffer is skipped (silent no-op).
    pub async fn complete_call(&self, call_id: &str, outcome: DelegationOutcome) {
        let task = {
            let mut inner = self.pending.inner.lock().await;
            match inner.running.remove(call_id) {
                Some(task) => {
                    // Atomic running → completed so a concurrent status query
                    // never sees the task as neither running nor completed.
                    let duration_ms = task.started_at.elapsed().as_millis() as u64;
                    inner.insert_completed(
                        call_id,
                        build_completed(
                            &task.parent_connection_id,
                            task.child_conversation_id,
                            task.agent_type,
                            duration_ms,
                            &outcome,
                        ),
                    );
                    Some((task, duration_ms))
                }
                None => {
                    // Buffer for the racing `start_delegation` to drain iff still
                    // reserved (mid-setup); a no-op otherwise, so the clone only
                    // materializes on the genuine pre-registration race.
                    inner.buffer_early_complete(call_id, outcome.clone());
                    None
                }
            }
        };
        if let Some((task, duration_ms)) = task {
            let _gate = match task.late_binding_gate.as_ref() {
                Some(gate) => Some(gate.lock().await),
                None => None,
            };
            self.finalize_delegation(
                &task.parent_connection_id,
                &task.parent_tool_use_id,
                &task.child_connection_id,
                task.child_conversation_id,
                task.agent_type,
                duration_ms,
                &outcome,
            )
            .await;
            self.result_notify.notify_waiters();
        }
    }

    /// Write the terminal meta, emit `DelegationCompleted`, and tear down the
    /// child for a resolved delegation. Shared by `complete_call` and
    /// `start_delegation`'s early-terminal pickup. Mirrors the resolution onto
    /// the parent's `delegate_to_agent` ToolCallState meta (including a bounded
    /// `text_preview` on the completed path so a post-refresh snapshot renders
    /// the result inline) so snapshot recovery shows the final state without the
    /// live `delegation_completed` event. Does not touch the pending maps — the
    /// caller owns the `running` → `completed` migration.
    ///
    /// `duration_ms` is the broker-measured elapsed time (from `started_at`),
    /// carried onto the event summary so the parent UI shows a real duration.
    #[allow(clippy::too_many_arguments)]
    async fn finalize_delegation(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        child_connection_id: &str,
        child_conversation_id: i32,
        agent_type: AgentType,
        duration_ms: u64,
        outcome: &DelegationOutcome,
    ) {
        let meta = match outcome {
            DelegationOutcome::Ok(ok) => build_delegation_meta(
                "completed",
                Some(child_connection_id),
                Some(child_conversation_id),
                None,
                build_text_preview(&ok.text).as_deref(),
                Some(duration_ms),
            ),
            DelegationOutcome::Err { code, .. } => build_delegation_meta(
                "failed",
                Some(child_connection_id),
                Some(child_conversation_id),
                Some(code),
                None,
                Some(duration_ms),
            ),
        };
        self.write_meta_if_real(parent_connection_id, parent_tool_use_id, meta)
            .await;
        self.emit_completed_if_real(
            parent_connection_id,
            parent_tool_use_id,
            child_connection_id,
            child_conversation_id,
            agent_type,
            outcome_to_summary(outcome, duration_ms),
        )
        .await;
        // v1 one-shot: always tear down the child.
        let _ = self.spawner.disconnect(child_connection_id).await;
    }

    /// Internal helper — apply the meta write iff the parent's
    /// `tool_use_id` refers to a real ACP `tool_call_id`. The
    /// broker-synthesized `"delegation-<uuid>"` placeholder targets no
    /// ToolCallState, so emitting a `ToolCallUpdate` against it would be
    /// noise that the frontend would route through `apply_tool_call_update`
    /// to a non-existent entry. See `meta_writer::is_synthetic_parent_tool_use_id`.
    async fn write_meta_if_real(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        meta: serde_json::Value,
    ) {
        if is_synthetic_parent_tool_use_id(parent_tool_use_id) {
            return;
        }
        self.meta_writer
            .write_meta(parent_connection_id, parent_tool_use_id, meta)
            .await;
    }

    /// Internal helper — emit `AcpEvent::DelegationStarted` on the parent's
    /// stream iff the `parent_tool_use_id` refers to a real ACP tool_call.
    /// Mirror of `emit_completed_if_real`: same synthetic-id skip, and the
    /// event rides the parent's stream so the frontend `DelegationContext`
    /// receives it via the parent's per-connection attach stream in
    /// web/server mode (not only via the desktop firehose).
    async fn emit_started_if_real(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        child_connection_id: &str,
        child_conversation_id: i32,
        agent_type: AgentType,
    ) {
        if is_synthetic_parent_tool_use_id(parent_tool_use_id) {
            return;
        }
        self.event_emitter
            .emit_started(
                parent_connection_id,
                parent_tool_use_id,
                child_connection_id,
                child_conversation_id,
                agent_type,
            )
            .await;
    }

    /// Internal helper — emit `AcpEvent::DelegationCompleted` on the parent's
    /// stream iff the `parent_tool_use_id` refers to a real ACP tool_call.
    /// Synthetic ids (the `"delegation-<uuid>"` UUID fallback) map to no
    /// live UI binding, so the emit would be wasted noise — same skip
    /// criterion as `write_meta_if_real`.
    async fn emit_completed_if_real(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        child_connection_id: &str,
        child_conversation_id: i32,
        agent_type: AgentType,
        result: DelegationResultSummary,
    ) {
        if is_synthetic_parent_tool_use_id(parent_tool_use_id) {
            return;
        }
        self.event_emitter
            .emit_completed(
                parent_connection_id,
                parent_tool_use_id,
                child_connection_id,
                child_conversation_id,
                agent_type,
                result,
            )
            .await;
    }

    /// Cancel the pending delegation whose parent and `external_handle` match.
    /// Called by the MCP listener on receipt of `notifications/cancelled`
    /// from a companion. When no matching pending entry exists (the
    /// cancel arrived before `handle_request` reached the
    /// pending-registration phase) the handle is stashed in
    /// `pre_canceled_handles` so the in-flight request can drain itself
    /// when it tries to register or shortly after.
    pub async fn cancel_by_external_handle(
        &self,
        parent_connection_id: &str,
        external_handle: &str,
        reason: String,
    ) {
        let (drained, bindings) = {
            let mut inner = self.pending.inner.lock().await;
            let bindings: Vec<(String, String)> = inner
                .running
                .iter()
                .filter(|(_, task)| {
                    task.parent_connection_id == parent_connection_id
                        && task.external_handle.as_deref() == Some(external_handle)
                })
                .map(|(call_id, task)| (call_id.clone(), task.parent_connection_id.clone()))
                .collect();
            let keys = bindings
                .iter()
                .map(|(call_id, _)| call_id.clone())
                .collect();
            (
                drain_and_record_canceled(&mut inner, keys, &reason),
                bindings,
            )
        };
        if drained.is_empty() {
            // Race: the cancel beat the handle's `running` registration. Buffer
            // it (capped, FIFO-evicted) so `start_delegation` can drain itself on
            // the next checkpoint instead of proceeding to spawn the child.
            self.buffer_pre_canceled_handle(parent_connection_id, external_handle.to_string())
                .await;
            return;
        }
        for (call_id, parent_connection_id) in bindings {
            self.cancel_late_tool_call_binding(&parent_connection_id, &call_id)
                .await;
        }
        for (task, duration_ms) in drained {
            // A turn is in flight, so cancel + disconnect.
            self.teardown_canceled_child(&task, duration_ms, true).await;
        }
        self.result_notify.notify_waiters();
    }

    /// Resolve the pending delegation whose child matches
    /// `child_connection_id` with a `canceled` outcome. Used when a child
    /// session disconnects or errors out without firing a clean
    /// TurnComplete — the parent's `tool_use_id` shouldn't dangle.
    /// No-op when no matching entry exists.
    ///
    /// `terminal_error` carries the child connection's last `AcpEvent::Error`
    /// detail when the lifecycle worker is dispatching off an `Error` event
    /// (vs. a bare `Disconnected`). When present, it gets appended to the
    /// `Canceled { reason }` string so the parent agent's tool-call result
    /// surfaces the real cause (e.g. "Authentication required",
    /// "transport closed") instead of the opaque default. Falls back to
    /// the default reason when `None`.
    pub async fn cancel_by_child_connection(
        &self,
        child_connection_id: &str,
        terminal_error: Option<&str>,
    ) {
        let reason = child_canceled_reason(terminal_error);
        let (drained, bindings) = {
            let mut inner = self.pending.inner.lock().await;
            let bindings: Vec<(String, String)> = inner
                .running
                .iter()
                .filter(|(_, v)| v.child_connection_id == child_connection_id)
                .map(|(call_id, task)| (call_id.clone(), task.parent_connection_id.clone()))
                .collect();
            let drained = if bindings.is_empty() {
                // No running entry. If the child is still reserved,
                // `start_delegation` is mid-setup and this failure beat the
                // `running` insert — buffer its detail for it to drain at
                // registration instead of no-oping. `buffer_child_failure` is a
                // no-op when the child isn't reserved, so a normal
                // post-resolution child teardown accumulates nothing.
                inner.buffer_child_failure(
                    child_connection_id,
                    terminal_error.map(|s| s.to_string()),
                );
                Vec::new()
            } else {
                let keys = bindings
                    .iter()
                    .map(|(call_id, _)| call_id.clone())
                    .collect();
                drain_and_record_canceled(&mut inner, keys, &reason)
            };
            (drained, bindings)
        };
        for (call_id, parent_connection_id) in bindings {
            self.cancel_late_tool_call_binding(&parent_connection_id, &call_id)
                .await;
        }
        for (task, duration_ms) in drained {
            // The child already disconnected/errored — disconnect-only teardown
            // (no spawner `cancel`, there's no live turn to interrupt).
            self.teardown_canceled_child(&task, duration_ms, false)
                .await;
        }
        self.result_notify.notify_waiters();
    }

    /// Cascade-cancel every pending delegation owned by `parent_connection_id`
    /// when the parent **connection tears down** (disconnect / `run_connection`
    /// exit). Drops the parent's entire tool_call tracker bucket (`pending` +
    /// `consumed`) since the connection is going away. Runs fully inline — the
    /// connection is already exiting, so there is no next prompt to unblock.
    pub async fn cancel_by_parent(&self, parent_connection_id: &str) {
        self.drain_for_parent_cancel(parent_connection_id, false)
            .await;
        let _ = self.spawn_parent_cancel_worker(parent_connection_id).await;
    }

    /// Cascade-cancel every pending delegation owned by `parent_connection_id`
    /// for a **turn/prompt cancel** where the parent connection STAYS ALIVE
    /// (a non-`end_turn` turn end, or a user Cancel between/within prompts).
    ///
    /// The fast, turn-scoped part — tombstoning the tool_call tracker and
    /// removing this parent's parked calls — runs SYNCHRONOUSLY: the caller
    /// awaits it before the connection loop accepts the next prompt, so it can't
    /// race a next-turn registration and tombstone/cancel that turn's legitimate
    /// entries (the safety the `drop_tool_calls_for_parent` invariant relies
    /// on). Only the slow child teardown (meta/emit + spawner `cancel` /
    /// `disconnect`, which can block on slow agents) is backgrounded, so the
    /// user-visible Cancel path stays responsive.
    ///
    /// RETAINS the parent's `consumed` tool_call memory (and tombstones the
    /// cancelled turn's unclaimed `pending` ids into it): dropping it would let
    /// a host re-emit of an already-handled `tool_call_id` re-register and
    /// mis-bind the next same-key delegation on this live connection — see
    /// `drop_tool_calls_for_parent`.
    pub async fn cancel_by_parent_turn(&self, parent_connection_id: &str) {
        self.drain_for_parent_cancel(parent_connection_id, true)
            .await;
        // Dropping a Tokio JoinHandle detaches the owned worker. Its queued
        // teardown remains broker-visible until every phase completes.
        drop(self.spawn_parent_cancel_worker(parent_connection_id));
    }

    fn spawn_parent_cancel_worker(
        &self,
        parent_connection_id: &str,
    ) -> tokio::task::JoinHandle<()> {
        let broker = self.clone();
        let parent_connection_id = parent_connection_id.to_string();
        tokio::spawn(async move {
            broker.finalize_parent_cancel(&parent_connection_id).await;
        })
    }

    /// Fast, lock-guarded part of a parent cancel: drop/tombstone this parent's
    /// tool_call tracker (per `keep_consumed`, see `drop_tool_calls_for_parent`)
    /// and remove every running task it owns, queueing them for the (slow)
    /// child teardown. Touches only the two broker mutexes — no spawner I/O — so
    /// it is safe to await inline in the connection loop before the next prompt
    /// is accepted.
    ///
    /// `keep_consumed` also governs the completed-cache: a **turn** cancel
    /// (`true`) records each drained task as `Canceled` so the still-alive
    /// connection's LLM can still query it; a **connection teardown** (`false`)
    /// drops the parent's whole completed-cache instead — the parent is gone, so
    /// nothing will query it.
    async fn drain_for_parent_cancel(&self, parent_connection_id: &str, keep_consumed: bool) {
        {
            // Lock order is tracker -> pending, shared with late-binding arm.
            // This makes tracker cleanup and running-task cancellation one
            // atomic handoff: an old setup cannot recreate a binding afterward.
            let mut tool_calls = self.tool_calls.inner.lock().await;
            let mut inner = self.pending.inner.lock().await;
            let binding_call_ids = drop_tool_calls_for_parent_locked(
                &mut tool_calls,
                parent_connection_id,
                keep_consumed,
            );
            for call_id in binding_call_ids {
                inner.clear_late_binding_state(&call_id);
            }
            // Flag every still-in-flight setup this parent owns in the SAME lock
            // acquisition that drains its running tasks: a delegation is then
            // caught either here (mid-setup → `start_delegation` tears its child
            // down at the next checkpoint) or by the running drain below (already
            // registered) — there is no interleaving where both miss it.
            inner.mark_inflight_canceled_for_parent(parent_connection_id);
            let keys: Vec<String> = inner
                .running
                .iter()
                .filter(|(_, v)| v.parent_connection_id == parent_connection_id)
                .map(|(k, _)| k.clone())
                .collect();
            if keep_consumed {
                // Turn cancel: connection stays alive → keep each canceled
                // result queryable.
                inner.enqueue_turn_canceled_tasks(keys);
            } else {
                // Connection teardown: just remove the running tasks and drop the
                // whole completed-cache for this parent. No completed entry to
                // match, but still capture the elapsed once (at drain time) so
                // the teardown meta doesn't recompute it later.
                inner.enqueue_connection_canceled_tasks(parent_connection_id, keys);
            }
        }
        if !keep_consumed {
            self.drop_pre_canceled_handles_for_parent(parent_connection_id)
                .await;
        }
        self.result_notify.notify_waiters();
        self.teardown_notify.notify_waiters();
    }

    /// Slow part of a parent cancel: for each drained task, patch the parent
    /// meta, emit `DelegationCompleted`, and tear the child down. The canceled
    /// result was already recorded into `completed` (turn cancel) by
    /// `drain_for_parent_cancel` under the lock, so this is pure I/O. Split out
    /// so a turn cancel can background it without delaying the fast, turn-scoped
    /// drain.
    async fn finalize_parent_cancel(&self, parent_connection_id: &str) {
        loop {
            let notified = self.teardown_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (claimed, still_pending) = {
                let mut inner = self.pending.inner.lock().await;
                let claimed = inner.claim_canceled_teardown(parent_connection_id);
                let still_pending = inner.has_canceled_teardown_for_parent(parent_connection_id);
                (claimed, still_pending)
            };
            match claimed {
                Some((call_id, teardown, processing)) => {
                    let _lease = TeardownProcessingLease {
                        _processing: processing,
                        notify: Arc::clone(&self.teardown_notify),
                    };
                    self.finalize_canceled_teardown_step(&call_id, &teardown)
                        .await;
                }
                None if still_pending => notified.await,
                None => break,
            }
        }
    }

    async fn finalize_canceled_teardown_step(&self, call_id: &str, teardown: &CanceledTeardown) {
        match teardown.phase {
            TeardownPhase::Meta => self.finalize_canceled_meta(call_id, teardown).await,
            TeardownPhase::Event => self.finalize_canceled_event(call_id, teardown).await,
            TeardownPhase::Cancel => self.finalize_canceled_cancel(call_id, teardown).await,
            TeardownPhase::Disconnect => self.finalize_canceled_disconnect(call_id, teardown).await,
        }
    }

    async fn finalize_canceled_meta(&self, call_id: &str, teardown: &CanceledTeardown) {
        let _gate = match teardown.task.late_binding_gate.as_ref() {
            Some(gate) => Some(gate.lock().await),
            None => None,
        };
        self.write_meta_if_real(
            &teardown.task.parent_connection_id,
            &teardown.task.parent_tool_use_id,
            build_delegation_meta(
                "failed",
                Some(&teardown.task.child_connection_id),
                Some(teardown.task.child_conversation_id),
                Some("canceled"),
                None,
                Some(teardown.duration_ms),
            ),
        )
        .await;
        let mut inner = self.pending.inner.lock().await;
        inner.advance_canceled_teardown(call_id, TeardownPhase::Event);
    }

    async fn finalize_canceled_event(&self, call_id: &str, teardown: &CanceledTeardown) {
        let _gate = match teardown.task.late_binding_gate.as_ref() {
            Some(gate) => Some(gate.lock().await),
            None => None,
        };
        self.emit_completed_if_real(
            &teardown.task.parent_connection_id,
            &teardown.task.parent_tool_use_id,
            &teardown.task.child_connection_id,
            teardown.task.child_conversation_id,
            teardown.task.agent_type,
            DelegationResultSummary::Err {
                error_code: "canceled".to_string(),
            },
        )
        .await;
        let mut inner = self.pending.inner.lock().await;
        inner.advance_canceled_teardown(call_id, TeardownPhase::Cancel);
    }

    async fn finalize_canceled_cancel(&self, call_id: &str, teardown: &CanceledTeardown) {
        if let Err(error) = self
            .spawner
            .cancel(&teardown.task.child_connection_id)
            .await
        {
            tracing::warn!(
                child_connection_id = %teardown.task.child_connection_id,
                error = %error,
                "[delegation] parent-canceled child cancel request failed; disconnecting anyway"
            );
        }
        let mut inner = self.pending.inner.lock().await;
        inner.advance_canceled_teardown(call_id, TeardownPhase::Disconnect);
    }

    async fn finalize_canceled_disconnect(&self, call_id: &str, teardown: &CanceledTeardown) {
        if let Err(error) = self
            .spawner
            .disconnect(&teardown.task.child_connection_id)
            .await
        {
            tracing::warn!(
                child_connection_id = %teardown.task.child_connection_id,
                error = %error,
                "[delegation] parent-canceled child disconnect reported an error"
            );
        }
        let mut inner = self.pending.inner.lock().await;
        inner.finish_canceled_teardown(call_id);
    }

    /// Shared canceled-child teardown: best-effort `failed`/`canceled` meta
    /// patch (so a parent-side snapshot post-cancel shows the delegation as
    /// canceled rather than stuck on "running"), a `DelegationCompleted` err
    /// event, then child teardown. `cancel_turn` is `true` when a turn is in
    /// flight (cancel + disconnect) and `false` when the child already
    /// disconnected/errored (disconnect only). Does NOT touch the pending maps —
    /// the caller already migrated the task into `completed`.
    ///
    /// `duration_ms` is the elapsed captured by `drain_and_record_canceled` at
    /// drain time — reused here (not recomputed) so the parent-card meta matches
    /// the completed-cache duration the status/cancel cards report, even when
    /// this teardown is backgrounded.
    async fn teardown_canceled_child(
        &self,
        task: &RunningTask,
        duration_ms: u64,
        cancel_turn: bool,
    ) {
        let _gate = match task.late_binding_gate.as_ref() {
            Some(gate) => Some(gate.lock().await),
            None => None,
        };
        self.write_meta_if_real(
            &task.parent_connection_id,
            &task.parent_tool_use_id,
            build_delegation_meta(
                "failed",
                Some(&task.child_connection_id),
                Some(task.child_conversation_id),
                Some("canceled"),
                None,
                Some(duration_ms),
            ),
        )
        .await;
        self.emit_completed_if_real(
            &task.parent_connection_id,
            &task.parent_tool_use_id,
            &task.child_connection_id,
            task.child_conversation_id,
            task.agent_type,
            DelegationResultSummary::Err {
                error_code: "canceled".to_string(),
            },
        )
        .await;
        if cancel_turn {
            let _ = self.spawner.cancel(&task.child_connection_id).await;
        }
        let _ = self.spawner.disconnect(&task.child_connection_id).await;
    }

    /// Backs the `get_delegation_status` tool for a single task id — a thin
    /// wrapper over [`Self::get_tasks_status`] so the single- and batch-poll
    /// paths share one snapshot/wait implementation. A one-id batch's
    /// "any task settled" wake condition is exactly "this task settled", so the
    /// blocking semantics are identical to the historical single-task loop.
    pub async fn get_task_status(
        &self,
        parent_connection_id: &str,
        parent_conversation_id: Option<i32>,
        task_id: &str,
        wait: StatusWait,
    ) -> DelegationTaskReport {
        let ids = [task_id.to_string()];
        self.get_tasks_status(parent_connection_id, parent_conversation_id, &ids, wait)
            .await
            .pop()
            .unwrap_or_else(|| unknown_report(task_id))
    }

    /// Backs the batch `get_delegation_status` tool. Resolves the status of one
    /// or many task ids in a single pass — each from the completed-cache, then
    /// the running set, then the DB fallback — scoped to the calling parent (a
    /// task owned by another parent reports `Unknown`, never leaking it). Returns
    /// one report per requested id, in request order.
    ///
    /// Blocking obeys [`StatusWait`]: `Immediate` returns the first snapshot.
    /// `Bounded`/`Infinite` return as soon as ANY requested task is terminal —
    /// INCLUDING one already terminal at entry, so a completed result is never
    /// held hostage to a long-running sibling (the caller re-polls the
    /// still-running ids to collect the rest). Only an all-running batch parks:
    /// it wakes when a task settles (the running count drops below the total) or
    /// — for `Bounded` — when the deadline elapses. An all-settled batch returns
    /// immediately even under `Infinite`, so it never parks forever.
    pub async fn get_tasks_status(
        &self,
        parent_connection_id: &str,
        parent_conversation_id: Option<i32>,
        task_ids: &[String],
        wait: StatusWait,
    ) -> Vec<DelegationTaskReport> {
        if task_ids.is_empty() {
            return Vec::new();
        }
        // A bounded wait gets a single fixed deadline; Immediate and Infinite
        // carry none — Immediate returns on the first pass, Infinite parks on
        // `result_notify` until a task is terminal.
        let deadline = match wait {
            StatusWait::Bounded(ms) => Some(Instant::now() + Duration::from_millis(ms)),
            StatusWait::Immediate | StatusWait::Infinite => None,
        };
        loop {
            // Arm the notify BEFORE the snapshot so a completion landing between
            // the snapshot and the await isn't lost (enable() registers now).
            let notified = self.result_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            // One lock acquisition classifies every requested id. The async
            // resolution of running (live reply) / not-in-memory (DB) ids is
            // deferred to `assemble_reports`, OUTSIDE this lock.
            let classes: Vec<StatusClass> = {
                let inner = self.pending.inner.lock().await;
                task_ids
                    .iter()
                    .map(|id| classify_locked(&inner, parent_connection_id, id))
                    .collect()
            };
            let running_count = classes
                .iter()
                .filter(|c| matches!(c, StatusClass::Running { .. }))
                .count();

            // Return now when the poll is Immediate, OR when at least one
            // requested task is already (or now) terminal — i.e. not EVERY task
            // is still running. This honors the contract "returns as soon as ANY
            // requested task reaches a terminal state": a mixed [terminal,
            // running] batch surfaces the terminal report immediately instead of
            // holding it hostage to a long-running sibling, and the caller
            // re-polls (narrowing to the still-running ids) to collect the rest.
            // `running_count == 0` (all settled) is the special case that also
            // makes Infinite safe. The id set is fixed and a task can only LEAVE
            // the running map during a wait (never (re)enter), so once a parked
            // all-running batch is woken by a settle the count has dropped below
            // the total and this returns; a spurious wake (another parent's task)
            // re-snapshots all-running and re-parks.
            if matches!(wait, StatusWait::Immediate) || running_count < task_ids.len() {
                return self
                    .assemble_reports(parent_conversation_id, task_ids, classes)
                    .await;
            }
            // Every requested task is still running. A `Bounded` wait gives up at
            // its deadline and returns the running snapshot; `Infinite` parks on
            // the notify alone.
            let now = Instant::now();
            if deadline.is_some_and(|d| now >= d) {
                return self
                    .assemble_reports(parent_conversation_id, task_ids, classes)
                    .await;
            }
            // Park until the next completion signal, bounded by the deadline
            // when there is one (Infinite waits on the notify alone).
            match deadline {
                Some(d) => {
                    let remaining = d - now;
                    tokio::select! {
                        _ = &mut notified => {}
                        _ = tokio::time::sleep(remaining) => {}
                    }
                }
                None => {
                    notified.await;
                }
            }
            // Loop: re-snapshot (a task likely just completed, or the deadline
            // passed and the next pass returns the running snapshot).
        }
    }

    /// Finish a batch status pass: resolve each [`StatusClass`] into a final
    /// report AFTER the pending lock is released. `Running` ids get their latest
    /// live reply attached; `NotInMemory` ids fall back to the DB status lookup.
    /// Reports come back in `task_ids` order.
    async fn assemble_reports(
        &self,
        parent_conversation_id: Option<i32>,
        task_ids: &[String],
        classes: Vec<StatusClass>,
    ) -> Vec<DelegationTaskReport> {
        let mut out = Vec::with_capacity(classes.len());
        for (id, class) in task_ids.iter().zip(classes) {
            let report = match class {
                StatusClass::Settled(report) => report,
                StatusClass::Running {
                    mut report,
                    child_connection_id,
                } => {
                    self.attach_live_reply(&mut report, &child_connection_id)
                        .await;
                    report
                }
                StatusClass::NotInMemory => self.status_from_db(parent_conversation_id, id).await,
            };
            out.push(report);
        }
        out
    }

    /// Upgrade a running report's bare `"Running."` message with the child's
    /// latest one-line activity, so the parent LLM gets a concrete sign of
    /// progress it can report in one shot (instead of polling-and-narrating).
    /// Called only on the actual running-return paths, AFTER the pending lock is
    /// released. A no-op when the lookup has nothing (default Noop lookup, child
    /// gone, or no live output yet) — the report stays `"Running."`.
    ///
    /// The hint goes on its OWN line (`"Running.\nLatest sub-agent reply: …"`),
    /// not appended to the marker line. On hosts that persist only the
    /// `CallToolResult` content text (e.g. Claude Code), the frontend recognizes
    /// a still-running poll by the standalone first line `"Running."` — keeping
    /// the child-controlled reply text on a separate line means a *completed*
    /// result that merely starts with "Running. …" can never be misread as
    /// running. See `textRunningStatus` in `src/lib/delegation-status.ts`.
    async fn attach_live_reply(
        &self,
        report: &mut DelegationTaskReport,
        child_connection_id: &str,
    ) {
        if let Some(reply) = self
            .live_reply_lookup
            .latest_reply(child_connection_id)
            .await
        {
            report.message = Some(format!("Running.\nLatest sub-agent reply: {reply}"));
        }
    }

    /// Backs the `cancel_delegation` tool. Cancels a running task owned by the
    /// caller (recording it `Canceled` + tearing the child down) and returns the
    /// resulting report. A task that already finished returns its terminal
    /// report; one not in memory falls back to the DB status (a finished task
    /// can't be canceled). Parent-scoped like `get_task_status`.
    pub async fn cancel_task_by_id(
        &self,
        parent_connection_id: &str,
        parent_conversation_id: Option<i32>,
        task_id: &str,
    ) -> DelegationTaskReport {
        let drained = {
            let mut inner = self.pending.inner.lock().await;
            if let Some(c) = inner.completed.get(task_id) {
                if c.parent_connection_id == parent_connection_id {
                    return completed_report(task_id, c);
                }
                return unknown_report(task_id);
            }
            match inner.running.get(task_id) {
                Some(r) if r.parent_connection_id == parent_connection_id => {
                    drain_and_record_canceled(
                        &mut inner,
                        vec![task_id.to_string()],
                        "canceled by request",
                    )
                    .pop()
                }
                Some(_) => return unknown_report(task_id),
                None => None,
            }
        };
        match drained {
            Some((task, duration_ms)) => {
                self.cancel_late_tool_call_binding(parent_connection_id, task_id)
                    .await;
                // A turn is in flight → cancel + disconnect. Reuse the duration
                // captured at drain time for both the teardown meta and the
                // report, so all three (completed-cache, meta, report) agree.
                self.teardown_canceled_child(&task, duration_ms, true).await;
                self.result_notify.notify_waiters();
                report_from_outcome(
                    Some(task_id.to_string()),
                    Some(task.agent_type),
                    &canceled_outcome(task.child_conversation_id, "canceled by request"),
                    Some(duration_ms),
                )
            }
            None => self.status_from_db(parent_conversation_id, task_id).await,
        }
    }

    /// DB status fallback for a task evicted from / never in the in-memory maps.
    /// Scopes to the caller's conversation: a child whose `parent_id` doesn't
    /// match (or when the caller has no active conversation) reports `Unknown`.
    async fn status_from_db(
        &self,
        parent_conversation_id: Option<i32>,
        task_id: &str,
    ) -> DelegationTaskReport {
        match self.status_lookup.find_by_call_id(task_id).await {
            Some(rec)
                if parent_conversation_id.is_some() && rec.parent_id == parent_conversation_id =>
            {
                db_report(task_id, &rec)
            }
            _ => unknown_report(task_id),
        }
    }
}

/// `ConversationDepthLookup` over the live `AppDatabase`. Used by the
/// production wiring; tests use the in-module `MockDepth`.
pub struct DbDepthLookup {
    pub db: Arc<crate::db::AppDatabase>,
}

#[async_trait]
impl ConversationDepthLookup for DbDepthLookup {
    async fn parent_of(&self, conversation_id: i32) -> Result<Option<i32>, DelegationError> {
        use sea_orm::EntityTrait;
        let row = crate::db::entities::conversation::Entity::find_by_id(conversation_id)
            .one(&self.db.conn)
            .await
            .map_err(|e| DelegationError::SubagentRuntimeError(format!("db: {e}")))?;
        Ok(row.and_then(|r| r.parent_id))
    }
}

/// `ChildStatusLookup` over the live `AppDatabase`. Recovers a delegation
/// task's terminal status (NOT its text — child output isn't in iyw-claw's DB)
/// from the child conversation row once its in-memory result was evicted.
pub struct DbChildStatusLookup {
    pub db: Arc<crate::db::AppDatabase>,
}

#[async_trait]
impl ChildStatusLookup for DbChildStatusLookup {
    async fn find_by_call_id(&self, call_id: &str) -> Option<ChildStatusRecord> {
        let summary = crate::db::service::conversation_service::get_by_delegation_call_id(
            &self.db.conn,
            call_id,
        )
        .await
        .ok()
        .flatten()?;
        // `summary.status` is the serialized `ConversationStatus` string.
        let status = match summary.status.as_str() {
            "in_progress" => TaskStatus::Running,
            "pending_review" | "completed" => TaskStatus::Completed,
            "cancelled" => TaskStatus::Canceled,
            _ => TaskStatus::Unknown,
        };
        Some(ChildStatusRecord {
            child_conversation_id: summary.id,
            status,
            agent_type: summary.agent_type,
            parent_id: summary.parent_id,
        })
    }
}
