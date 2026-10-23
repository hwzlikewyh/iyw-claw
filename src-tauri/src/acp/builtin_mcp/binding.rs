use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use rmcp::transport::streamable_http_server::SessionId;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const DELIVERING: u8 = 0;
const AWAITING_CONFIRMATION: u8 = 1;
const CONFIRMED: u8 = 2;
const ABORTED: u8 = 3;
const REPLACED: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Principal([u8; 32]);

impl Principal {
    pub(super) fn from_bearer(bearer: &str) -> Self {
        Self(Sha256::digest(bearer.as_bytes()).into())
    }
}

struct SessionBinding {
    principal: Principal,
    parent_connection_id: String,
    phase: Arc<AtomicU8>,
    stream_cancellation: CancellationToken,
}

#[derive(Clone)]
pub(super) struct ProvisionalBinding {
    session_id: String,
    principal: Principal,
    phase: Arc<AtomicU8>,
}

impl ProvisionalBinding {
    pub(super) fn session_id(&self) -> &str {
        &self.session_id
    }
}

pub(super) enum BindProvisionalResult {
    Bound { ticket: ProvisionalBinding },
    Conflict,
    Cancelled,
}

#[derive(Default)]
pub(super) struct SessionBindings {
    inner: tokio::sync::RwLock<HashMap<String, SessionBinding>>,
}

impl SessionBindings {
    pub(super) async fn bind_provisional(
        &self,
        session_id: String,
        principal: Principal,
        parent_connection_id: String,
        cancellation: &CancellationToken,
    ) -> BindProvisionalResult {
        let mut entries = self.inner.write().await;
        if cancellation.is_cancelled() {
            return BindProvisionalResult::Cancelled;
        }
        if entries.contains_key(&session_id) {
            return BindProvisionalResult::Conflict;
        }
        // 同一授权可由主代理、子代理和探测客户端共用；新会话不能撤销旧请求。
        let phase = Arc::new(AtomicU8::new(DELIVERING));
        entries.insert(
            session_id.clone(),
            SessionBinding {
                principal,
                parent_connection_id,
                phase: Arc::clone(&phase),
                stream_cancellation: CancellationToken::new(),
            },
        );
        BindProvisionalResult::Bound {
            ticket: ProvisionalBinding {
                session_id,
                principal,
                phase,
            },
        }
    }

    pub(super) async fn confirm_and_authorize(
        &self,
        session_id: &str,
        principal: Principal,
    ) -> Option<CancellationToken> {
        let entries = self.inner.write().await;
        let Some(binding) = entries
            .get(session_id)
            .filter(|binding| binding.principal == principal)
        else {
            return None;
        };
        confirm_phase(&binding.phase).then(|| binding.stream_cancellation.clone())
    }

    pub(super) fn mark_delivered(ticket: &ProvisionalBinding) {
        let _ = ticket.phase.compare_exchange(
            DELIVERING,
            AWAITING_CONFIRMATION,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(super) fn mark_aborted(ticket: &ProvisionalBinding) -> bool {
        loop {
            let phase = ticket.phase.load(Ordering::Acquire);
            if !matches!(phase, DELIVERING | AWAITING_CONFIRMATION) {
                return false;
            }
            if ticket
                .phase
                .compare_exchange(phase, ABORTED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }

    pub(super) async fn remove_ticket(&self, ticket: &ProvisionalBinding) -> bool {
        let mut entries = self.inner.write().await;
        let matches = entries.get(&ticket.session_id).is_some_and(|binding| {
            binding.principal == ticket.principal && Arc::ptr_eq(&binding.phase, &ticket.phase)
        });
        if matches {
            if let Some(binding) = entries.remove(&ticket.session_id) {
                binding.stream_cancellation.cancel();
            }
        }
        matches
    }

    pub(super) async fn remove(&self, session_id: &str) {
        if let Some(binding) = self.inner.write().await.remove(session_id) {
            binding.stream_cancellation.cancel();
            binding.phase.store(REPLACED, Ordering::Release);
        }
    }

    pub(super) async fn remove_authorized(&self, session_id: &str, principal: Principal) -> bool {
        let mut entries = self.inner.write().await;
        let authorized = entries
            .get(session_id)
            .is_some_and(|binding| binding.principal == principal);
        if authorized {
            if let Some(binding) = entries.remove(session_id) {
                binding.stream_cancellation.cancel();
                binding.phase.store(REPLACED, Ordering::Release);
            }
        }
        authorized
    }

    pub(super) async fn principal_sessions(&self, principal: Principal) -> Vec<String> {
        self.inner
            .read()
            .await
            .iter()
            .filter(|(_, binding)| binding.principal == principal)
            .map(|(session_id, _)| session_id.clone())
            .collect()
    }

    pub(super) async fn parent_connection_ids(&self) -> Vec<String> {
        self.inner
            .read()
            .await
            .values()
            .map(|binding| binding.parent_connection_id.clone())
            .collect()
    }

    pub(super) async fn take_parent(&self, connection_id: &str) -> Vec<SessionId> {
        let mut entries = self.inner.write().await;
        take_matching(&mut entries, |binding| {
            binding.parent_connection_id == connection_id
        })
    }

    pub(super) async fn drain(&self) -> Vec<SessionId> {
        let mut entries = self.inner.write().await;
        take_matching(&mut entries, |_| true)
    }

    pub(super) async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

fn confirm_phase(phase: &AtomicU8) -> bool {
    loop {
        let current = phase.load(Ordering::Acquire);
        match current {
            CONFIRMED => return true,
            AWAITING_CONFIRMATION => {
                if phase
                    .compare_exchange(current, CONFIRMED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return true;
                }
            }
            DELIVERING | ABORTED | REPLACED => return false,
            _ => return false,
        }
    }
}

fn take_matching(
    entries: &mut HashMap<String, SessionBinding>,
    matches: impl Fn(&SessionBinding) -> bool,
) -> Vec<SessionId> {
    let mut removed = Vec::new();
    entries.retain(|session_id, binding| {
        if !matches(binding) {
            return true;
        }
        binding.stream_cancellation.cancel();
        binding.phase.store(REPLACED, Ordering::Release);
        removed.push(Arc::<str>::from(session_id.as_str()));
        false
    });
    removed
}
