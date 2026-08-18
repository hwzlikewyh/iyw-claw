use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rmcp::transport::streamable_http_server::session::local::LocalSessionHandle;
use rmcp::transport::streamable_http_server::SessionId;
use tokio::sync::{Mutex as AsyncMutex, Notify};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CleanupStep {
    Detaching,
    RetireTokens,
    CancelBroker,
    DropToolCalls,
    CancelQuestions,
    CancelConfirmations,
    CloseProtocolSessions,
    Complete,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum CleanupKey {
    Parent(String),
    ShutdownOrphans,
}

struct CleanupState {
    intent_revision: u64,
    revision: u64,
    step: CleanupStep,
    broker_tokens: HashSet<String>,
    protocol_sessions: HashMap<SessionId, Option<LocalSessionHandle>>,
    revoked_sessions: usize,
}

impl CleanupState {
    fn new() -> Self {
        Self {
            intent_revision: 1,
            revision: 1,
            step: CleanupStep::Detaching,
            broker_tokens: HashSet::new(),
            protocol_sessions: HashMap::new(),
            revoked_sessions: 0,
        }
    }

    fn reset_for_retry(&mut self) {
        self.intent_revision = self.intent_revision.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
        self.step = CleanupStep::Detaching;
    }
}

struct CleanupEntry {
    key: CleanupKey,
    state: Mutex<CleanupState>,
    runner: AsyncMutex<()>,
    worker_active: AtomicBool,
    changed: Arc<Notify>,
}

#[derive(Clone)]
pub(super) struct CleanupTicket {
    entry: Arc<CleanupEntry>,
}

pub(super) struct CleanupSnapshot {
    pub(super) revision: u64,
    pub(super) step: CleanupStep,
    pub(super) protocol_sessions: Vec<(SessionId, LocalSessionHandle)>,
    pub(super) protocol_session_count: usize,
}

#[derive(Default)]
pub(super) struct PendingCleanupRegistry {
    entries: Mutex<HashMap<CleanupKey, Arc<CleanupEntry>>>,
    changed: Arc<Notify>,
}

impl PendingCleanupRegistry {
    pub(super) fn enqueue_parent(&self, connection_id: &str) -> CleanupTicket {
        self.enqueue(CleanupKey::Parent(connection_id.to_string()))
    }

    pub(super) fn enqueue_orphans(&self) -> CleanupTicket {
        self.enqueue(CleanupKey::ShutdownOrphans)
    }

    pub(super) fn tickets(&self) -> Vec<CleanupTicket> {
        self.lock_entries()
            .values()
            .cloned()
            .map(|entry| CleanupTicket { entry })
            .collect()
    }

    pub(super) fn parent_ids(&self) -> Vec<String> {
        self.lock_entries()
            .keys()
            .filter_map(|key| match key {
                CleanupKey::Parent(connection_id) => Some(connection_id.clone()),
                CleanupKey::ShutdownOrphans => None,
            })
            .collect()
    }

    pub(super) fn contains(&self, ticket: &CleanupTicket) -> bool {
        self.lock_entries()
            .get(&ticket.entry.key)
            .is_some_and(|entry| Arc::ptr_eq(entry, &ticket.entry))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.lock_entries().is_empty()
    }

    pub(super) fn change_notifier(&self) -> Arc<Notify> {
        Arc::clone(&self.changed)
    }

    pub(super) fn complete(&self, ticket: &CleanupTicket, expected_revision: u64) -> bool {
        let mut entries = self.lock_entries();
        let Some(current) = entries.get(&ticket.entry.key) else {
            return true;
        };
        if !Arc::ptr_eq(current, &ticket.entry) {
            return true;
        }
        let state = ticket.entry.lock_state();
        let complete = state.revision == expected_revision
            && state.step == CleanupStep::Complete
            && state.protocol_sessions.is_empty();
        drop(state);
        if complete {
            entries.remove(&ticket.entry.key);
            signal(&self.changed);
        }
        complete
    }

    fn enqueue(&self, key: CleanupKey) -> CleanupTicket {
        let mut entries = self.lock_entries();
        let entry = entries
            .entry(key.clone())
            .and_modify(|entry| entry.lock_state().reset_for_retry())
            .or_insert_with(|| {
                Arc::new(CleanupEntry {
                    key,
                    state: Mutex::new(CleanupState::new()),
                    runner: AsyncMutex::new(()),
                    worker_active: AtomicBool::new(false),
                    changed: Arc::clone(&self.changed),
                })
            })
            .clone();
        signal(&self.changed);
        CleanupTicket { entry }
    }

    fn lock_entries(&self) -> MutexGuard<'_, HashMap<CleanupKey, Arc<CleanupEntry>>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CleanupTicket {
    pub(super) fn parent_id(&self) -> Option<&str> {
        match &self.entry.key {
            CleanupKey::Parent(connection_id) => Some(connection_id),
            CleanupKey::ShutdownOrphans => None,
        }
    }

    pub(super) async fn lock_runner(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.entry.runner.lock().await
    }

    pub(super) fn try_start_worker(&self) -> bool {
        self.entry
            .worker_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(super) fn finish_worker(&self) {
        self.entry.worker_active.store(false, Ordering::Release);
        signal(&self.entry.changed);
    }

    pub(super) fn detach_revision(&self) -> u64 {
        self.entry.lock_state().intent_revision
    }

    pub(super) fn add_broker_token(&self, token: String) {
        let mut state = self.entry.lock_state();
        if state.broker_tokens.insert(token) {
            state.revision = state.revision.wrapping_add(1);
        }
    }

    pub(super) fn add_protocol_ids(&self, ids: impl IntoIterator<Item = SessionId>) {
        let mut state = self.entry.lock_state();
        for id in ids {
            if let Entry::Vacant(entry) = state.protocol_sessions.entry(id) {
                entry.insert(None);
                state.revision = state.revision.wrapping_add(1);
            }
        }
    }

    pub(super) fn attach_protocol_handle(&self, id: SessionId, handle: LocalSessionHandle) {
        let mut state = self.entry.lock_state();
        state.protocol_sessions.insert(id, Some(handle));
        state.revision = state.revision.wrapping_add(1);
    }

    pub(super) fn finish_protocol_session(&self, id: &SessionId) {
        let mut state = self.entry.lock_state();
        if state.protocol_sessions.remove(id).is_some() {
            state.revision = state.revision.wrapping_add(1);
        }
    }

    pub(super) fn protocol_ids(&self) -> Vec<SessionId> {
        self.entry
            .lock_state()
            .protocol_sessions
            .keys()
            .cloned()
            .collect()
    }

    pub(super) fn mark_detached(&self, expected_intent: u64, revoked: usize) -> bool {
        let mut state = self.entry.lock_state();
        state.revoked_sessions = state.revoked_sessions.saturating_add(revoked);
        if state.intent_revision != expected_intent {
            return false;
        }
        state.step = if self.parent_id().is_some() {
            CleanupStep::RetireTokens
        } else {
            CleanupStep::CloseProtocolSessions
        };
        state.revision = state.revision.wrapping_add(1);
        signal(&self.entry.changed);
        true
    }

    pub(super) fn snapshot(&self) -> CleanupSnapshot {
        let state = self.entry.lock_state();
        let protocol_session_count = state.protocol_sessions.len();
        CleanupSnapshot {
            revision: state.revision,
            step: state.step,
            protocol_sessions: state
                .protocol_sessions
                .iter()
                .filter_map(|(id, handle)| Some((id.clone(), handle.clone()?)))
                .collect(),
            protocol_session_count,
        }
    }

    pub(super) fn advance(&self, snapshot: &CleanupSnapshot, next: CleanupStep) -> bool {
        let mut state = self.entry.lock_state();
        if state.revision != snapshot.revision || state.step != snapshot.step {
            return false;
        }
        if state.step == CleanupStep::RetireTokens {
            state.broker_tokens.clear();
        }
        state.step = next;
        state.revision = state.revision.wrapping_add(1);
        signal(&self.entry.changed);
        true
    }

    pub(super) fn revoked_sessions(&self) -> usize {
        self.entry.lock_state().revoked_sessions
    }
}

impl CleanupEntry {
    fn lock_state(&self) -> MutexGuard<'_, CleanupState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn signal(notify: &Notify) {
    notify.notify_waiters();
    notify.notify_one();
}
