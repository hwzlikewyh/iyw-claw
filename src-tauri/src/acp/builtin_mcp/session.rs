use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

mod state;

use super::authority::{SessionAuthority, SessionContext};
use super::credential::{digest_token, mint_token, SessionToken, TokenDigest};
use state::{cancel_contexts, prune_revoked_parents, record_revoked_parent, take_matching};

const MAX_SESSION_AUTHORITIES: usize = 1024;
const REVOKED_PARENT_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_REVOKED_PARENTS: usize = 2048;

#[derive(Debug, thiserror::Error)]
pub enum SessionIssueError {
    #[error("the MCP session registry is closed")]
    RegistryClosed,
    #[error("the parent ACP connection has been revoked")]
    ParentRevoked,
    #[error("the MCP session authority capacity has been reached")]
    CapacityReached,
    #[error("failed to generate an MCP session credential")]
    Entropy(#[source] rand::Error),
}

struct RegistryState {
    accepting: bool,
    revoked_parents: HashMap<String, Instant>,
    sessions: HashMap<TokenDigest, SessionRecord>,
}

struct SessionRecord {
    context: SessionContext,
}

impl SessionRecord {
    fn new(context: SessionContext) -> Self {
        Self { context }
    }
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            accepting: true,
            revoked_parents: HashMap::new(),
            sessions: HashMap::new(),
        }
    }
}

pub struct SessionRegistry {
    inner: RwLock<RegistryState>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self {
            inner: RwLock::new(RegistryState::default()),
        }
    }
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn issue(
        self: &Arc<Self>,
        authority: SessionAuthority,
    ) -> Result<(SessionToken, SessionContext), SessionIssueError> {
        let context = SessionContext::new(authority);
        loop {
            let (token, digest) = match mint_token() {
                Ok(minted) => minted,
                Err(error) => {
                    context.cancel();
                    return Err(error);
                }
            };
            if self.insert(digest, &context).await? {
                tracing::info!(target: "builtin_mcp", connection_id = %context.connection_id(),
                    agent_type = %context.agent_type(), "issued HTTP MCP session authority");
                return Ok((token, context));
            }
        }
    }

    pub async fn lookup(&self, token: &str) -> Option<SessionContext> {
        let digest = digest_token(token)?;
        self.inner
            .read()
            .await
            .sessions
            .get(&digest)
            .map(|record| record.context.clone())
            .filter(|context| !context.cancellation().is_cancelled())
    }

    pub(super) async fn parent_connection_ids(&self) -> Vec<String> {
        let state = self.inner.read().await;
        state
            .sessions
            .values()
            .map(|record| record.context.connection_id().to_string())
            .collect()
    }

    pub(super) async fn is_closed_and_empty(&self) -> bool {
        let state = self.inner.read().await;
        !state.accepting && state.sessions.is_empty()
    }

    pub async fn revoke_parent(&self, connection_id: &str) -> usize {
        let contexts = {
            let mut state = self.inner.write().await;
            let contexts = take_matching(&mut state.sessions, connection_id);
            record_revoked_parent(&mut state.revoked_parents, connection_id, Instant::now());
            contexts
        };
        let count = cancel_contexts(contexts);
        tracing::info!(target: "builtin_mcp", connection_id, revoked_sessions = count,
            "revoked HTTP MCP sessions for parent connection");
        count
    }

    /// Permanently close issuance, reject lookup, and cancel every session.
    pub async fn revoke_all(&self) -> usize {
        let contexts = {
            let mut state = self.inner.write().await;
            state.accepting = false;
            state.revoked_parents.clear();
            state
                .sessions
                .drain()
                .map(|(_, record)| record.context)
                .collect()
        };
        let count = cancel_contexts(contexts);
        tracing::info!(target: "builtin_mcp", revoked_sessions = count,
            "closed HTTP MCP session registry");
        count
    }

    async fn insert(
        &self,
        digest: TokenDigest,
        context: &SessionContext,
    ) -> Result<bool, SessionIssueError> {
        let mut state = self.inner.write().await;
        prune_revoked_parents(&mut state.revoked_parents, Instant::now());
        if !state.accepting {
            context.cancel();
            return Err(SessionIssueError::RegistryClosed);
        }
        if state.revoked_parents.contains_key(context.connection_id()) {
            context.cancel();
            return Err(SessionIssueError::ParentRevoked);
        }
        if state.sessions.contains_key(&digest) {
            return Ok(false);
        }
        if state.sessions.len() >= MAX_SESSION_AUTHORITIES {
            context.cancel();
            return Err(SessionIssueError::CapacityReached);
        }
        state
            .sessions
            .insert(digest, SessionRecord::new(context.clone()));
        Ok(true)
    }
}
