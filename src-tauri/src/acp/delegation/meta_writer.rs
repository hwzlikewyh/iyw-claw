//! `DelegationMetaWriter` — broker capability that attaches the live
//! delegation state onto the parent's active `delegate_to_agent`
//! tool-call. The shape written under `meta["iyw-claw.delegation"]`
//! follows the convention documented at
//! [`crate::acp::session_state::ToolCallState::meta`].
//!
//! The broker calls this at three lifecycle points:
//!
//! 1. After `send_prompt_linked_for_delegation` returns Ok — sets
//!    `status: "running"` with the child's connection / conversation ids.
//! 2. In `complete_call` — sets `status: "completed"` (ok branch) or
//!    `status: "failed"` + `error_code` (err branch).
//! 3. In `cancel_by_parent` / `cancel_by_child_connection` — sets
//!    `status: "failed"` + `error_code: "canceled"`.
//!
//! Writes are skipped when the broker is operating on a synthetic
//! `parent_tool_use_id` (the `"delegation-*"` UUID fallback) because
//! there's no matching ACP `tool_call_id` to attach meta to. The
//! frontend's snapshot path will still recover via `parseInput(input)`.

use async_trait::async_trait;
use std::sync::Arc;

use crate::acp::manager::ConnectionManager;
use crate::acp::types::AcpEvent;
use crate::web::event_bridge::emit_with_state;

/// Top-level key under which delegation state lives on a tool call's
/// `meta` object. Single source of truth — both the writer and the
/// frontend reader must spell it the same way.
pub const DELEGATION_META_KEY: &str = "iyw-claw.delegation";

/// Capability the broker uses to patch `meta["iyw-claw.delegation"]` on
/// the parent connection's active `delegate_to_agent` tool call.
///
/// Errors are swallowed at the impl boundary: a missing parent
/// connection (e.g. user disconnected mid-delegation) or a stale
/// tool_use_id (e.g. parent turn already wrapped up) must not derail
/// the rest of the broker lifecycle, which still has to disconnect the
/// child and resolve the pending call.
#[async_trait]
pub trait DelegationMetaWriter: Send + Sync {
    async fn write_meta(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        meta: serde_json::Value,
    );
}

/// Default writer used when the broker is constructed via the
/// short-form `DelegationBroker::new` (most test callsites). Silently
/// drops every write — the broker's correctness is observable through
/// its outcomes and pending-call accounting, not through meta emits.
#[derive(Default, Clone)]
pub struct NoopMetaWriter;

#[async_trait]
impl DelegationMetaWriter for NoopMetaWriter {
    async fn write_meta(
        &self,
        _parent_connection_id: &str,
        _parent_tool_use_id: &str,
        _meta: serde_json::Value,
    ) {
    }
}

/// Production impl backed by `ConnectionManager`. Emits an
/// `AcpEvent::ToolCallUpdate` carrying only the `meta` field so the
/// existing `apply_tool_call_update` merge path (partial-update
/// preservation of locations / images / content / etc.) is reused
/// without duplicating the patch logic.
#[derive(Clone)]
pub struct ConnectionManagerMetaWriter {
    pub manager: Arc<ConnectionManager>,
}

#[async_trait]
impl DelegationMetaWriter for ConnectionManagerMetaWriter {
    async fn write_meta(
        &self,
        parent_connection_id: &str,
        parent_tool_use_id: &str,
        meta: serde_json::Value,
    ) {
        let Some((state_arc, emitter)) = self
            .manager
            .get_state_and_emitter(parent_connection_id)
            .await
        else {
            return;
        };
        emit_with_state(
            &state_arc,
            &emitter,
            AcpEvent::ToolCallUpdate {
                tool_call_id: parent_tool_use_id.to_string(),
                title: None,
                status: None,
                content: None,
                raw_input: None,
                raw_output: None,
                raw_output_append: None,
                locations: None,
                meta: Some(meta),
                images: None,
            },
        )
        .await;
    }
}

/// Helper to construct the canonical `meta["iyw-claw.delegation"]` value.
/// Keeps the schema in one place so the writer impls and the broker
/// callsites can't drift apart on field naming.
pub fn build_delegation_meta(
    status: &str,
    child_connection_id: Option<&str>,
    child_conversation_id: Option<i32>,
    error_code: Option<&str>,
    text_preview: Option<&str>,
    duration_ms: Option<u64>,
) -> serde_json::Value {
    let mut inner = serde_json::Map::new();
    inner.insert(
        "status".to_string(),
        serde_json::Value::String(status.to_string()),
    );
    if let Some(id) = child_connection_id {
        inner.insert(
            "child_connection_id".to_string(),
            serde_json::Value::String(id.to_string()),
        );
    }
    if let Some(id) = child_conversation_id {
        inner.insert(
            "child_conversation_id".to_string(),
            serde_json::Value::Number(serde_json::Number::from(id)),
        );
    }
    if let Some(code) = error_code {
        inner.insert(
            "error_code".to_string(),
            serde_json::Value::String(code.to_string()),
        );
    }
    // Inline result preview so a parent-side snapshot replay after a refresh can
    // render the completed result without the live `delegation_completed` event
    // (which carries the same preview). Only set on the terminal `completed`
    // write; `None` everywhere else.
    if let Some(preview) = text_preview {
        inner.insert(
            "text_preview".to_string(),
            serde_json::Value::String(preview.to_string()),
        );
    }
    // Carry the broker-measured elapsed time so a parent-side snapshot replay
    // after a refresh shows the execution duration without the live
    // `delegation_completed` event. Set on the terminal writes (completed /
    // failed / canceled); `None` for the running write — same survival semantics
    // as `text_preview` above. NOTE: the live event only carries duration on its
    // `Ok` summary, so for failed/canceled the duration is meta-only (the live
    // card shows none until refresh, when this meta supplies it).
    if let Some(ms) = duration_ms {
        inner.insert(
            "duration_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(ms)),
        );
    }
    let mut outer = serde_json::Map::new();
    outer.insert(
        DELEGATION_META_KEY.to_string(),
        serde_json::Value::Object(inner),
    );
    serde_json::Value::Object(outer)
}

/// True when the broker handed out a synthetic placeholder
/// `parent_tool_use_id` (no matching ACP tool_call_id exists). Skipping
/// meta writes for these avoids spamming `ToolCallUpdate` events with a
/// tool_call_id that no live `ToolCallState` will ever match.
pub fn is_synthetic_parent_tool_use_id(id: &str) -> bool {
    id.starts_with("delegation-")
}
