use std::collections::HashSet;
use std::sync::Arc;

use super::cleanup::{CleanupSnapshot, CleanupStep, CleanupTicket};
use super::{LeaseManager, LeaseShutdownReport};

impl LeaseManager {
    pub(super) async fn cleanup_parent(self: &Arc<Self>, connection_id: &str) -> usize {
        let ticket = self.pending_cleanup.enqueue_parent(connection_id);
        let before = ticket.revoked_sessions();
        self.drive_ticket(&ticket).await;
        ticket.revoked_sessions().saturating_sub(before)
    }

    pub(super) async fn begin_shutdown(self: &Arc<Self>) -> (usize, bool) {
        let revoked = self.enqueue_shutdown_cleanup().await;
        self.parent_tasks.abort_all().await;
        (revoked, self.parent_tasks.reap_all().await)
    }

    pub(super) async fn cleanup_all(self: &Arc<Self>) -> LeaseShutdownReport {
        let (mut revoked, mut parent_tasks_reaped) = self.begin_shutdown().await;
        self.drive_all_pending().await;
        self.cleanup_tasks.wait_idle().await;

        let (late_revoked, late_parent_tasks_reaped) = self.begin_shutdown().await;
        revoked = revoked.saturating_add(late_revoked);
        parent_tasks_reaped &= late_parent_tasks_reaped;
        self.drive_all_pending().await;
        self.cleanup_tasks.wait_idle().await;
        self.audit_shutdown(revoked, parent_tasks_reaped).await
    }

    async fn enqueue_shutdown_cleanup(&self) -> usize {
        let _lifecycle = self.lifecycle.lock().await;
        let mut parents = self.shutdown_parent_ids().await;
        parents.extend(self.pending_cleanup.parent_ids());
        for connection_id in parents {
            self.pending_cleanup.enqueue_parent(&connection_id);
        }
        self.pending_cleanup.enqueue_orphans();
        self.sessions.revoke_all().await
    }

    async fn shutdown_parent_ids(&self) -> HashSet<String> {
        let mut parents = self
            .sessions
            .parent_connection_ids()
            .await
            .into_iter()
            .collect::<HashSet<_>>();
        parents.extend(self.runtimes.connection_ids().await);
        parents.extend(self.bindings.parent_connection_ids().await);
        parents.extend(self.receipts.parent_connection_ids());
        parents
    }

    async fn drive_ticket(self: &Arc<Self>, ticket: &CleanupTicket) {
        let changed = self.pending_cleanup.change_notifier();
        while self.pending_cleanup.contains(ticket) {
            self.spawn_cleanup_worker(ticket.clone());
            let notified = changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !self.pending_cleanup.contains(ticket) {
                break;
            }
            notified.await;
        }
    }

    async fn drive_all_pending(self: &Arc<Self>) {
        let changed = self.pending_cleanup.change_notifier();
        loop {
            let tickets = self.pending_cleanup.tickets();
            if tickets.is_empty() {
                return;
            }
            for ticket in tickets {
                self.spawn_cleanup_worker(ticket);
            }
            let notified = changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.pending_cleanup.is_empty() {
                return;
            }
            notified.await;
        }
    }

    fn spawn_cleanup_worker(self: &Arc<Self>, ticket: CleanupTicket) {
        if !ticket.try_start_worker() {
            return;
        }
        let manager = Arc::downgrade(self);
        self.cleanup_tasks.spawn(async move {
            let _worker = TicketWorkerGuard(ticket.clone());
            if let Some(manager) = manager.upgrade() {
                manager.run_cleanup_ticket(&ticket).await;
            }
        });
    }

    async fn run_cleanup_ticket(self: &Arc<Self>, ticket: &CleanupTicket) {
        let _runner = ticket.lock_runner().await;
        while self.pending_cleanup.contains(ticket) {
            let snapshot = ticket.snapshot();
            if !self.run_cleanup_step(ticket, &snapshot).await {
                continue;
            }
            if snapshot.step == CleanupStep::Complete {
                if self.pending_cleanup.complete(ticket, snapshot.revision) {
                    return;
                }
            }
        }
    }

    async fn run_cleanup_step(&self, ticket: &CleanupTicket, snapshot: &CleanupSnapshot) -> bool {
        match snapshot.step {
            CleanupStep::Detaching => self.detach_resources(ticket).await,
            CleanupStep::RetireTokens => {
                self.broker_tokens
                    .revoke_by_parent(ticket.parent_id().expect("parent cleanup"))
                    .await;
                ticket.advance(snapshot, CleanupStep::CancelBroker)
            }
            CleanupStep::CancelBroker => {
                self.listener
                    .broker
                    .cancel_by_parent(ticket.parent_id().expect("parent cleanup"))
                    .await;
                ticket.advance(snapshot, CleanupStep::DropToolCalls)
            }
            CleanupStep::DropToolCalls => {
                self.listener
                    .broker
                    .drop_pending_tool_calls_for_parent(ticket.parent_id().expect("parent cleanup"))
                    .await;
                ticket.advance(snapshot, CleanupStep::CancelQuestions)
            }
            CleanupStep::CancelQuestions => {
                self.listener
                    .questions
                    .cancel_questions_by_parent(ticket.parent_id().expect("parent cleanup"))
                    .await;
                ticket.advance(snapshot, CleanupStep::CancelConfirmations)
            }
            CleanupStep::CancelConfirmations => {
                self.listener
                    .confirmations
                    .cancel_channel_confirmations_by_parent(
                        ticket.parent_id().expect("parent cleanup"),
                    )
                    .await;
                ticket.advance(snapshot, CleanupStep::CloseProtocolSessions)
            }
            CleanupStep::CloseProtocolSessions => {
                self.close_protocol_sessions(ticket, snapshot).await
            }
            CleanupStep::Complete => true,
        }
    }

    async fn detach_resources(&self, ticket: &CleanupTicket) -> bool {
        let intent_revision = ticket.detach_revision();
        let _lifecycle = self.lifecycle.lock().await;
        let revoked = match ticket.parent_id() {
            Some(connection_id) => self.detach_parent_locked(ticket, connection_id).await,
            None => {
                self.detach_orphans_locked(ticket).await;
                0
            }
        };
        ticket.mark_detached(intent_revision, revoked)
    }

    async fn detach_parent_locked(&self, ticket: &CleanupTicket, connection_id: &str) -> usize {
        let revoked = self.sessions.revoke_parent(connection_id).await;
        self.receipts.remove_parent(connection_id);
        if let Some(credential) = self.runtimes.remove(connection_id).await {
            ticket.add_broker_token(credential.broker_token().to_string());
        }
        let session_ids = self.bindings.take_parent(connection_id).await;
        ticket.add_protocol_ids(session_ids);
        self.detach_protocol_handles(ticket).await;
        revoked
    }

    async fn detach_orphans_locked(&self, ticket: &CleanupTicket) {
        let bound_sessions = self.bindings.drain().await;
        let mut sessions = self.protocol_sessions.sessions.write().await;
        for session_id in bound_sessions {
            if let Some(handle) = sessions.remove(&session_id) {
                ticket.attach_protocol_handle(session_id, handle);
            }
        }
        for (session_id, handle) in sessions.drain() {
            ticket.attach_protocol_handle(session_id, handle);
        }
    }

    async fn detach_protocol_handles(&self, ticket: &CleanupTicket) {
        let mut sessions = self.protocol_sessions.sessions.write().await;
        for session_id in ticket.protocol_ids() {
            match sessions.remove(&session_id) {
                Some(handle) => ticket.attach_protocol_handle(session_id, handle),
                None => ticket.finish_protocol_session(&session_id),
            }
        }
    }

    async fn close_protocol_sessions(
        &self,
        ticket: &CleanupTicket,
        snapshot: &CleanupSnapshot,
    ) -> bool {
        for (session_id, handle) in &snapshot.protocol_sessions {
            if let Err(error) = handle.close().await {
                tracing::warn!(
                    target: "builtin_mcp",
                    session_id = %session_id,
                    error = %error,
                    "HTTP MCP protocol session already stopped during durable cleanup"
                );
            }
            ticket.finish_protocol_session(session_id);
        }
        let current = ticket.snapshot();
        current.step != CleanupStep::CloseProtocolSessions
            || current.protocol_session_count == 0
                && ticket.advance(&current, CleanupStep::Complete)
    }

    async fn audit_shutdown(
        &self,
        revoked_sessions: usize,
        parent_tasks_reaped: bool,
    ) -> LeaseShutdownReport {
        LeaseShutdownReport {
            revoked_sessions,
            authorities_closed: self.sessions.is_closed_and_empty().await,
            runtimes_empty: self.runtimes.is_empty().await,
            bindings_empty: self.bindings.is_empty().await,
            receipts_empty: self.receipts.is_empty(),
            pending_cleanup_empty: self.pending_cleanup.is_empty(),
            protocol_sessions_empty: self.protocol_sessions.sessions.read().await.is_empty(),
            parent_tasks_reaped,
            cleanup_tasks_reaped: self.cleanup_tasks.is_idle() && !self.cleanup_tasks.take_failed(),
        }
    }
}

struct TicketWorkerGuard(CleanupTicket);

impl Drop for TicketWorkerGuard {
    fn drop(&mut self) {
        self.0.finish_worker();
    }
}
