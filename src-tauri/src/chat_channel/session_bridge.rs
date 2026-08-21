use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::Instant;

use tokio::sync::Mutex as AsyncMutex;

use crate::acp::types::PermissionOptionInfo;
use crate::chat_channel::types::{ChannelMessageTarget, SentMessageId};
use crate::models::agent::AgentType;

pub struct PendingPermission {
    pub request_id: String,
    pub tool_description: String,
    pub options: Vec<PermissionOptionInfo>,
    pub sent_message_id: Option<SentMessageId>,
}

pub struct ActiveSession {
    pub channel_id: i32,
    pub sender_id: String,
    pub target: ChannelMessageTarget,
    pub route_key: String,
    pub target_id: String,
    pub bind_on_start: bool,
    pub conversation_id: i32,
    pub connection_id: String,
    pub registration_generation: u64,
    /// External Agent session id this connection is attempting to restore.
    /// Retained for compare-and-swap cleanup when the failure event was
    /// observed before the bridge subscriber was attached.
    pub restoring_external_id: Option<String>,
    /// CAS baseline for the next `SessionStarted` emitted by this connection.
    /// Unlike `restoring_external_id`, this rolls forward after fork.
    pub expected_external_id: Option<String>,
    /// Set after a `SessionStarted` has been durably accepted for this bridge
    /// entry; only such entries are eligible for generation fallback.
    pub observed_session_id: Option<String>,
    pub agent_type: AgentType,
    pub content_buffer: String,
    pub tool_calls: Vec<String>,
    /// Stores raw_input by tool_call_id for detail extraction on completion.
    pub tool_call_inputs: HashMap<String, String>,
    /// `tool_call_id`s of delegations whose terminal result line was already
    /// rendered to the channel. The dedup marker for async delegation: the
    /// result can surface via the terminal `ToolCallUpdate` OR the
    /// `DelegationCompleted` event (whichever fires for a given task), and this
    /// set guarantees exactly one result line. Kept separate from
    /// `tool_call_inputs` because that map is re-populated by every `raw_input`
    /// update and so can't serve as a one-shot token. Cleared with the session.
    pub delegation_rendered: HashSet<String>,
    pub last_flushed: Instant,
    pub pending_prompt: Option<String>,
    /// Original user text retained only while an external Agent session is
    /// being restored. Cleared once the Agent accepts the user message.
    pub recovery_prompt: Option<String>,
    /// How many times the deferred kickoff has been retried (bounded, then
    /// surfaced as an explicit failure instead of retrying forever).
    pub pending_prompt_attempts: u32,
    /// End-to-end trace id of the inbound message that started this session,
    /// propagated to outbound log rows.
    pub trace_id: Option<String>,
    pub permission_pending: Option<PendingPermission>,
}

#[derive(Default)]
pub struct SessionBridge {
    sessions: HashMap<String, ActiveSession>,
    registrations: HashMap<String, u64>,
    next_registration: u64,
    route_gates: HashMap<(i32, String), Weak<AsyncMutex<()>>>,
}

pub enum RouteActivation {
    Missing,
    Superseded,
    Activated(Vec<ActiveSession>),
}

pub struct FallbackCandidate {
    pub connection_id: String,
    pub conversation_id: i32,
    pub channel_id: i32,
    pub route_key: String,
    pub target_id: String,
    pub session_id: String,
    pub sender_id: String,
    pub target: ChannelMessageTarget,
    pub pending_prompt: Option<String>,
}

impl SessionBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route_gate(&mut self, channel_id: i32, route_key: &str) -> Arc<AsyncMutex<()>> {
        self.route_gates.retain(|_, gate| gate.strong_count() > 0);
        let key = (channel_id, route_key.to_string());
        if let Some(gate) = self.route_gates.get(&key).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(AsyncMutex::new(()));
        self.route_gates.insert(key, Arc::downgrade(&gate));
        gate
    }

    pub async fn acquire_route_gate(
        bridge: &Arc<AsyncMutex<Self>>,
        channel_id: i32,
        route_key: &str,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let gate = {
            let mut guard = bridge.lock().await;
            guard.route_gate(channel_id, route_key)
        };
        gate.lock_owned().await
    }

    pub async fn register_serialized(
        bridge: &Arc<AsyncMutex<Self>>,
        connection_id: String,
        session: ActiveSession,
    ) -> u64 {
        let _gate_guard =
            Self::acquire_route_gate(bridge, session.channel_id, &session.route_key).await;
        let mut guard = bridge.lock().await;
        guard.register(connection_id, session)
    }

    fn register(&mut self, connection_id: String, session: ActiveSession) -> u64 {
        self.next_registration = self.next_registration.saturating_add(1);
        let mut session = session;
        session.registration_generation = self.next_registration;
        self.registrations
            .insert(connection_id.clone(), self.next_registration);
        self.sessions.insert(connection_id, session);
        self.next_registration
    }

    pub fn is_latest_route_generation(&self, connection_id: &str, generation: u64) -> bool {
        self.sessions
            .get(connection_id)
            .is_some_and(|session| session.registration_generation == generation)
            && self.is_latest_route_registration(connection_id)
    }

    pub fn is_failed_route_generation(&self, failed: &ActiveSession) -> bool {
        self.sessions
            .values()
            .filter(|session| {
                session.channel_id == failed.channel_id && session.route_key == failed.route_key
            })
            .all(|session| session.registration_generation <= failed.registration_generation)
    }

    pub fn remove(&mut self, connection_id: &str) -> Option<ActiveSession> {
        self.registrations.remove(connection_id);
        self.sessions.remove(connection_id)
    }

    pub fn get(&self, connection_id: &str) -> Option<&ActiveSession> {
        self.sessions.get(connection_id)
    }

    pub fn get_mut(&mut self, connection_id: &str) -> Option<&mut ActiveSession> {
        self.sessions.get_mut(connection_id)
    }

    pub fn activate_route(&mut self, connection_id: &str) -> RouteActivation {
        let Some(current) = self.sessions.get_mut(connection_id) else {
            return RouteActivation::Missing;
        };
        let channel_id = current.channel_id;
        let route_key = current.route_key.clone();
        let registration = self
            .registrations
            .get(connection_id)
            .copied()
            .unwrap_or_default();
        let superseded = self.sessions.iter().any(|(id, session)| {
            id != connection_id
                && session.channel_id == channel_id
                && session.route_key == route_key
                && self.registrations.get(id).copied().unwrap_or_default() > registration
        });
        if superseded {
            return RouteActivation::Superseded;
        }
        self.sessions
            .get_mut(connection_id)
            .expect("session exists")
            .bind_on_start = false;
        let replaced_ids = self
            .sessions
            .iter()
            .filter_map(|(id, session)| {
                (id != connection_id
                    && session.channel_id == channel_id
                    && session.route_key == route_key)
                    .then(|| id.clone())
            })
            .collect::<Vec<_>>();
        let replaced = replaced_ids
            .iter()
            .filter_map(|id| self.sessions.remove(id))
            .collect();
        for id in replaced_ids {
            self.registrations.remove(&id);
        }
        RouteActivation::Activated(replaced)
    }

    pub fn is_latest_route_registration(&self, connection_id: &str) -> bool {
        let Some(current) = self.sessions.get(connection_id) else {
            return false;
        };
        let current_registration = self
            .registrations
            .get(connection_id)
            .copied()
            .unwrap_or_default();
        self.sessions.iter().all(|(id, session)| {
            session.channel_id != current.channel_id
                || session.route_key != current.route_key
                || self.registrations.get(id).copied().unwrap_or_default() <= current_registration
        })
    }

    /// Promote the newest older connection that already observed a
    /// `SessionStarted`. Its id was persisted while it was superseded, so a
    /// newer failed generation can hand the route back without spawning a
    /// third process.
    pub fn fallback_candidate(&self, failed: &ActiveSession) -> Option<FallbackCandidate> {
        let candidate_id = self
            .sessions
            .iter()
            .filter(|(id, session)| {
                *id != &failed.connection_id
                    && session.channel_id == failed.channel_id
                    && session.route_key == failed.route_key
                    && session.registration_generation < failed.registration_generation
                    && session.observed_session_id.is_some()
            })
            .max_by_key(|(id, _)| self.registrations.get(*id).copied().unwrap_or_default())
            .map(|(id, _)| id.clone())?;
        let session = self.sessions.get(&candidate_id)?;
        Some(FallbackCandidate {
            connection_id: candidate_id,
            conversation_id: session.conversation_id,
            channel_id: session.channel_id,
            route_key: session.route_key.clone(),
            target_id: session.target_id.clone(),
            session_id: session.observed_session_id.clone()?,
            sender_id: session.sender_id.clone(),
            target: session.target.clone(),
            pending_prompt: session
                .pending_prompt
                .clone()
                .or_else(|| failed.pending_prompt.clone())
                .or_else(|| failed.recovery_prompt.clone()),
        })
    }

    pub fn activate_fallback(&mut self, candidate: &FallbackCandidate) -> bool {
        if !self.is_latest_route_registration(&candidate.connection_id) {
            return false;
        }
        let Some(session) = self.sessions.get_mut(&candidate.connection_id) else {
            return false;
        };
        if session.observed_session_id.as_deref() != Some(candidate.session_id.as_str()) {
            return false;
        }
        session.bind_on_start = false;
        session.expected_external_id = Some(candidate.session_id.clone());
        if session.pending_prompt.is_none() {
            session.pending_prompt = candidate.pending_prompt.clone();
        }
        true
    }

    pub fn find_by_sender(&self, channel_id: i32, sender_id: &str) -> Option<&ActiveSession> {
        self.sessions
            .values()
            .find(|s| s.channel_id == channel_id && s.sender_id == sender_id)
    }

    pub fn find_by_route(&self, channel_id: i32, route_key: &str) -> Option<&ActiveSession> {
        self.sessions
            .values()
            .filter(|session| session.channel_id == channel_id && session.route_key == route_key)
            .min_by_key(|session| {
                (
                    session.bind_on_start,
                    std::cmp::Reverse(
                        self.registrations
                            .get(&session.connection_id)
                            .copied()
                            .unwrap_or_default(),
                    ),
                )
            })
    }

    pub fn find_by_route_mut(
        &mut self,
        channel_id: i32,
        route_key: &str,
    ) -> Option<&mut ActiveSession> {
        self.sessions
            .values_mut()
            .filter(|session| session.channel_id == channel_id && session.route_key == route_key)
            .min_by_key(|session| session.bind_on_start)
    }

    pub fn find_by_target(&self, target: &ChannelMessageTarget) -> Option<&ActiveSession> {
        self.sessions
            .values()
            .find(|session| session.target.matches_thread(target))
    }

    pub fn find_by_sender_mut(
        &mut self,
        channel_id: i32,
        sender_id: &str,
    ) -> Option<&mut ActiveSession> {
        self.sessions
            .values_mut()
            .find(|s| s.channel_id == channel_id && s.sender_id == sender_id)
    }

    pub fn find_by_target_mut(
        &mut self,
        target: &ChannelMessageTarget,
    ) -> Option<&mut ActiveSession> {
        self.sessions
            .values_mut()
            .find(|session| session.target.matches_thread(target))
    }

    pub fn all_sessions(&self) -> impl Iterator<Item = &ActiveSession> {
        self.sessions.values()
    }

    pub fn all_sessions_mut(&mut self) -> impl Iterator<Item = &mut ActiveSession> {
        self.sessions.values_mut()
    }
}
