use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use tokio::sync::Mutex;

use crate::acp::delegation::listener::{DelegationListener, TokenEntry, TokenRegistry};
use crate::acp::memory_turn::MemoryTurnTracker;

use super::authority::{SessionAuthority, SessionContext};
use super::binding::SessionBindings;
use super::credential::SessionToken;
use super::receipt::DeliveryReceiptRegistry;
use super::runtime::{RuntimeCredential, RuntimeRegistry};
use super::session::{SessionIssueError, SessionRegistry};

mod cleanup;
mod tasks;
mod teardown;

use cleanup::PendingCleanupRegistry;
use tasks::{CleanupTasks, ParentRevocationTasks};

pub(super) struct LeaseShutdownReport {
    pub(super) revoked_sessions: usize,
    pub(super) authorities_closed: bool,
    pub(super) runtimes_empty: bool,
    pub(super) bindings_empty: bool,
    pub(super) receipts_empty: bool,
    pub(super) pending_cleanup_empty: bool,
    pub(super) protocol_sessions_empty: bool,
    pub(super) parent_tasks_reaped: bool,
    pub(super) cleanup_tasks_reaped: bool,
}

impl LeaseShutdownReport {
    pub(super) fn is_complete(&self) -> bool {
        self.authorities_closed
            && self.runtimes_empty
            && self.bindings_empty
            && self.receipts_empty
            && self.pending_cleanup_empty
            && self.protocol_sessions_empty
            && self.parent_tasks_reaped
            && self.cleanup_tasks_reaped
    }

    pub(super) fn merge_prelude(&mut self, revoked: usize, parent_tasks_reaped: bool) {
        self.revoked_sessions = self.revoked_sessions.saturating_add(revoked);
        self.parent_tasks_reaped &= parent_tasks_reaped;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuiltinMcpIssueError {
    #[error("the process HTTP MCP service is unavailable")]
    ServiceUnavailable,
    #[error("the parent connection already has HTTP MCP authority")]
    ParentAlreadyIssued,
    #[error(transparent)]
    Session(#[from] SessionIssueError),
}

pub(super) struct LeaseManager {
    sessions: Arc<SessionRegistry>,
    protocol_sessions: Arc<LocalSessionManager>,
    bindings: Arc<SessionBindings>,
    runtimes: Arc<RuntimeRegistry>,
    broker_tokens: Arc<TokenRegistry>,
    listener: Arc<DelegationListener>,
    receipts: DeliveryReceiptRegistry,
    lifecycle: Arc<Mutex<()>>,
    parent_tasks: ParentRevocationTasks,
    pending_cleanup: PendingCleanupRegistry,
    cleanup_tasks: Arc<CleanupTasks>,
}

impl LeaseManager {
    pub(super) fn new(listener: Arc<DelegationListener>) -> Arc<Self> {
        Arc::new(Self {
            sessions: Arc::new(SessionRegistry::new()),
            protocol_sessions: Arc::new(LocalSessionManager::default()),
            bindings: Arc::new(SessionBindings::default()),
            runtimes: Arc::new(RuntimeRegistry::default()),
            broker_tokens: Arc::clone(&listener.tokens),
            listener,
            receipts: DeliveryReceiptRegistry::default(),
            lifecycle: Arc::new(Mutex::new(())),
            parent_tasks: ParentRevocationTasks::default(),
            pending_cleanup: PendingCleanupRegistry::default(),
            cleanup_tasks: Arc::new(CleanupTasks::default()),
        })
    }

    pub(super) fn sessions(&self) -> Arc<SessionRegistry> {
        Arc::clone(&self.sessions)
    }

    pub(super) fn protocol_sessions(&self) -> Arc<LocalSessionManager> {
        Arc::clone(&self.protocol_sessions)
    }

    pub(super) fn bindings(&self) -> Arc<SessionBindings> {
        Arc::clone(&self.bindings)
    }

    pub(super) fn runtimes(&self) -> Arc<RuntimeRegistry> {
        Arc::clone(&self.runtimes)
    }

    pub(super) fn receipts(&self) -> DeliveryReceiptRegistry {
        self.receipts.clone()
    }

    pub(super) fn lifecycle(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.lifecycle)
    }

    pub(super) async fn issue(
        self: &Arc<Self>,
        authority: SessionAuthority,
        turn_tracker: Arc<MemoryTurnTracker>,
        ready: &AtomicBool,
    ) -> Result<SessionToken, BuiltinMcpIssueError> {
        let connection_id = authority.connection_id().to_string();
        let (result, cleanup_needed) = {
            let _lifecycle = self.lifecycle.lock().await;
            if !ready.load(Ordering::Acquire) {
                authority.cancellation().cancel();
                return Err(BuiltinMcpIssueError::ServiceUnavailable);
            }
            self.issue_locked(authority, turn_tracker, ready).await
        };
        if cleanup_needed {
            self.cleanup_parent(&connection_id).await;
        }
        result
    }

    pub(super) async fn revoke_parent(self: &Arc<Self>, connection_id: &str) -> usize {
        self.cleanup_parent(connection_id).await
    }

    pub(super) async fn begin_revoke_all(self: &Arc<Self>) -> (usize, bool) {
        self.begin_shutdown().await
    }

    pub(super) async fn revoke_all(self: &Arc<Self>) -> LeaseShutdownReport {
        self.cleanup_all().await
    }

    async fn issue_locked(
        self: &Arc<Self>,
        authority: SessionAuthority,
        turn_tracker: Arc<MemoryTurnTracker>,
        ready: &AtomicBool,
    ) -> (Result<SessionToken, BuiltinMcpIssueError>, bool) {
        let connection_id = authority.connection_id().to_string();
        let broker_token = uuid::Uuid::new_v4().to_string();
        let entry = token_entry(&authority, &connection_id, &broker_token, turn_tracker);
        let credential = RuntimeCredential::new(broker_token.clone());
        if !self
            .runtimes
            .insert_if_absent(connection_id.clone(), credential)
            .await
        {
            authority.cancellation().cancel();
            return (Err(BuiltinMcpIssueError::ParentAlreadyIssued), false);
        }
        let (bearer, context) = match self.issue_authority(authority, &connection_id).await {
            Ok(issued) => issued,
            Err(error) => return (Err(error), false),
        };
        self.broker_tokens.register(broker_token, entry).await;
        if !ready.load(Ordering::Acquire) || context.cancellation().is_cancelled() {
            context.cancel();
            self.pending_cleanup.enqueue_parent(&connection_id);
            return (Err(BuiltinMcpIssueError::ServiceUnavailable), true);
        }
        self.spawn_parent_revocation(&context).await;
        (Ok(bearer), false)
    }

    async fn spawn_parent_revocation(self: &Arc<Self>, context: &SessionContext) {
        let manager = Arc::downgrade(self);
        let cancellation = context.cancellation().clone();
        let connection_id = context.connection_id().to_string();
        self.parent_tasks
            .spawn(async move {
                cancellation.cancelled().await;
                if let Some(manager) = manager.upgrade() {
                    manager.revoke_parent(&connection_id).await;
                }
            })
            .await;
    }

    async fn issue_authority(
        &self,
        authority: SessionAuthority,
        connection_id: &str,
    ) -> Result<(SessionToken, SessionContext), BuiltinMcpIssueError> {
        match self.sessions.issue(authority).await {
            Ok(issued) => Ok(issued),
            Err(error) => {
                self.runtimes.remove(connection_id).await;
                Err(error.into())
            }
        }
    }
}

fn token_entry(
    authority: &SessionAuthority,
    connection_id: &str,
    broker_token: &str,
    turn_tracker: Arc<MemoryTurnTracker>,
) -> TokenEntry {
    let memory = authority.memory_permissions();
    let working_dir = authority.cwd().to_path_buf();
    let memory_workspace_key = crate::commands::skill_inventory::workspace_key(Some(
        working_dir.to_string_lossy().as_ref(),
    ));
    TokenEntry {
        parent_connection_id: connection_id.to_string(),
        working_dir,
        memory_workspace_key,
        agent_type: authority.agent_type(),
        memory_write_enabled: memory.append_enabled(),
        memory_proposal_enabled: memory.proposal_enabled(),
        memory_recall_enabled: memory.recall_enabled(),
        memory_documents_read_enabled: memory.documents_read_enabled(),
        opaque_source_id: crate::acp::memory_turn::derive_opaque_source_id(
            broker_token,
            connection_id,
        ),
        memory_turn_tracker: turn_tracker,
        cancellation: authority.cancellation().clone(),
        mutation_gate: crate::acp::delegation::mutation_gate::MutationGate::new(),
    }
}
