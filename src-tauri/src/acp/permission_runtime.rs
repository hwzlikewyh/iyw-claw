use std::sync::Arc;

use sacp::schema::RequestPermissionResponse;
use sacp::Responder;
use tokio::sync::{Mutex, RwLock};

use crate::acp::permission_queue::{PermissionAdmission, PermissionQueue, QueuedPermission};
use crate::acp::session_state::SessionState;
use crate::acp::types::AcpEvent;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

pub(crate) type PendingPermissions =
    Arc<Mutex<PermissionQueue<Responder<RequestPermissionResponse>>>>;

pub(crate) struct PermissionRequestMeta<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) tool_call_id: &'a str,
    pub(crate) option_count: usize,
    pub(crate) allow_option_count: usize,
}

pub(crate) struct PermissionRuntime<'a> {
    state: &'a Arc<RwLock<SessionState>>,
    emitter: &'a EventEmitter,
    permissions: &'a PendingPermissions,
}

impl<'a> PermissionRuntime<'a> {
    pub(crate) fn new(
        state: &'a Arc<RwLock<SessionState>>,
        emitter: &'a EventEmitter,
        permissions: &'a PendingPermissions,
    ) -> Self {
        Self {
            state,
            emitter,
            permissions,
        }
    }

    pub(crate) async fn admit(
        &self,
        responder: Responder<RequestPermissionResponse>,
        card: QueuedPermission,
        meta: PermissionRequestMeta<'_>,
    ) {
        let mut queue = self.permissions.lock().await;
        let request_id = card.request_id.clone();
        let admission = queue.admit(responder, card);
        let connection_id = self.state.read().await.connection_id.clone();
        match admission {
            PermissionAdmission::Visible(card) => {
                log_visible(&connection_id, &card, &meta);
                self.emit_card(card, 0).await;
            }
            PermissionAdmission::Queued => {
                let depth = queue.waiting_len() as u32;
                tracing::info!(
                    connection_id,
                    session_id = meta.session_id,
                    request_id,
                    tool_call_id = meta.tool_call_id,
                    visible_request_id = queue.visible_request_id().unwrap_or(""),
                    waiting = depth,
                    "[ACP] permission queued"
                );
                emit_with_state(
                    self.state,
                    self.emitter,
                    AcpEvent::PermissionQueueDepth { depth },
                )
                .await;
            }
            PermissionAdmission::Closed { delivery_failed } => {
                tracing::info!(
                    connection_id,
                    session_id = meta.session_id,
                    request_id,
                    tool_call_id = meta.tool_call_id,
                    delivery_failed,
                    "[ACP] permission rejected after connection teardown"
                );
            }
        }
    }

    pub(crate) async fn resolve(&self, request_id: String, option_id: String) {
        let mut queue = self.permissions.lock().await;
        let response = response_context(self.state, &request_id, &option_id).await;
        let resolved = queue.resolve(&request_id, option_id);
        if !resolved.answered {
            tracing::warn!(
                connection_id = response.connection_id,
                request_id,
                visible_request_id = queue.visible_request_id().unwrap_or(""),
                pending = queue.len(),
                "[ACP] permission response ignored"
            );
            return;
        }
        log_resolution(&request_id, &response, resolved.delivery_failed);
        if let Some(card) = resolved.next {
            let depth = queue.waiting_len() as u32;
            tracing::info!(
                connection_id = response.connection_id,
                request_id = card.request_id,
                previous_request_id = request_id,
                waiting = depth,
                "[ACP] permission promoted"
            );
            self.emit_card(card, depth).await;
        }
        emit_with_state(
            self.state,
            self.emitter,
            AcpEvent::PermissionResolved { request_id },
        )
        .await;
    }

    pub(crate) async fn drain(&self, reason: &'static str) {
        let mut queue = self.permissions.lock().await;
        self.drain_locked(&mut queue, reason, false).await;
    }

    pub(crate) async fn close_and_drain(&self, reason: &'static str) {
        let mut queue = self.permissions.lock().await;
        self.drain_locked(&mut queue, reason, true).await;
    }

    pub(crate) async fn drain_then_emit(&self, reason: &'static str, follow_up: AcpEvent) {
        let mut queue = self.permissions.lock().await;
        self.drain_locked(&mut queue, reason, false).await;
        emit_with_state(self.state, self.emitter, follow_up).await;
    }

    async fn emit_card(&self, card: QueuedPermission, queued: u32) {
        emit_with_state(
            self.state,
            self.emitter,
            AcpEvent::PermissionRequest {
                request_id: card.request_id,
                tool_call: card.tool_call,
                options: card.options,
                queued,
            },
        )
        .await;
    }

    async fn drain_locked(
        &self,
        queue: &mut PermissionQueue<Responder<RequestPermissionResponse>>,
        reason: &'static str,
        close: bool,
    ) {
        let drained = if close {
            queue.close_and_drain()
        } else {
            queue.drain()
        };
        if drained.count == 0 {
            return;
        }
        let connection_id = self.state.read().await.connection_id.clone();
        tracing::info!(
            connection_id,
            reason,
            count = drained.count,
            delivery_failures = drained.delivery_failures,
            "[ACP] pending permissions cancelled"
        );
        if let Some(request_id) = drained.visible_request_id {
            emit_with_state(
                self.state,
                self.emitter,
                AcpEvent::PermissionResolved { request_id },
            )
            .await;
        }
    }
}

struct PermissionResponseContext {
    connection_id: String,
    option_kind: String,
    wait_ms: i64,
}

async fn response_context(
    state: &Arc<RwLock<SessionState>>,
    request_id: &str,
    option_id: &str,
) -> PermissionResponseContext {
    let snapshot = state.read().await;
    let pending = snapshot
        .pending_permission
        .as_ref()
        .filter(|pending| pending.request_id == request_id);
    let option_kind = pending
        .and_then(|pending| {
            pending
                .options
                .iter()
                .find(|option| option.option_id == option_id)
        })
        .map(|option| option.kind.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let wait_ms = pending
        .map(|pending| {
            (chrono::Utc::now() - pending.created_at)
                .num_milliseconds()
                .max(0)
        })
        .unwrap_or_default();
    PermissionResponseContext {
        connection_id: snapshot.connection_id.clone(),
        option_kind,
        wait_ms,
    }
}

fn log_visible(connection_id: &str, card: &QueuedPermission, meta: &PermissionRequestMeta<'_>) {
    tracing::info!(
        connection_id,
        session_id = meta.session_id,
        request_id = card.request_id,
        tool_call_id = meta.tool_call_id,
        option_count = meta.option_count,
        allow_option_count = meta.allow_option_count,
        "[ACP] permission requested"
    );
}

fn log_resolution(request_id: &str, response: &PermissionResponseContext, delivery_failed: bool) {
    if delivery_failed {
        tracing::warn!(
            connection_id = response.connection_id,
            request_id,
            option_kind = response.option_kind,
            wait_ms = response.wait_ms,
            "[ACP] permission response delivery failed"
        );
        return;
    }
    tracing::info!(
        connection_id = response.connection_id,
        request_id,
        option_kind = response.option_kind,
        wait_ms = response.wait_ms,
        "[ACP] permission resolved"
    );
}
