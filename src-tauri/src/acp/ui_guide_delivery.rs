use super::types::{AcpEvent, ConnectionStatus, UserMessageBlock};

/// 仅在同一原生会话成功完成后复用指南；不持久化，重连默认补齐。
#[derive(Debug, Default)]
pub(crate) struct UiGuideDelivery {
    confirmed_session: Option<String>,
    pending: Option<(i64, String)>,
}

impl UiGuideDelivery {
    pub(crate) fn begin(&mut self, generation: i64, session: Option<&str>, reusable: bool) -> bool {
        self.pending = None;
        let Some(session) = session.filter(|id| !id.is_empty() && reusable) else {
            self.invalidate("reuse_unavailable");
            return true;
        };
        if self.confirmed_session.as_deref() == Some(session) {
            return false;
        }
        self.invalidate("session_changed");
        self.pending = Some((generation, session.to_owned()));
        true
    }

    pub(crate) fn invalidate(&mut self, reason: &'static str) {
        if self.confirmed_session.is_some() || self.pending.is_some() {
            tracing::info!(
                reason,
                had_confirmed = self.confirmed_session.is_some(),
                had_pending = self.pending.is_some(),
                "[IntelligentUI] guide delivery invalidated"
            );
        }
        self.confirmed_session = None;
        self.pending = None;
    }

    pub(crate) fn observe(&mut self, event: &AcpEvent, generation: i64) {
        match event {
            // ContentRecovered 仅回填展示投影，不改变原生模型历史。
            AcpEvent::SessionStarted { .. } => {
                self.invalidate("session_started");
            }
            AcpEvent::UserMessage { blocks, .. }
                if blocks.iter().any(|block| {
                    matches!(block,
                UserMessageBlock::Text { text } if text.trim_start().starts_with('/'))
                }) =>
            {
                self.invalidate("control_input");
            }
            AcpEvent::StatusChanged {
                status: ConnectionStatus::Disconnected | ConnectionStatus::Error,
            }
            | AcpEvent::Error { terminal: true, .. } => self.invalidate("connection_failed"),
            AcpEvent::ToolCall { meta, .. } | AcpEvent::ToolCallUpdate { meta, .. } => {
                if meta.as_ref().is_some_and(is_compaction) {
                    self.invalidate("compaction");
                }
            }
            AcpEvent::TurnComplete {
                session_id,
                stop_reason,
                ..
            } => self.finish(generation, (session_id, stop_reason)),
            _ => {}
        }
    }

    fn finish(&mut self, generation: i64, result: (&str, &str)) {
        if result.1 != "end_turn" {
            self.invalidate("turn_incomplete");
            return;
        }
        if let Some((pending_generation, session)) = self.pending.take() {
            if pending_generation == generation && session == result.0 {
                self.confirmed_session = Some(session);
                tracing::info!(generation, "[IntelligentUI] full guide delivery confirmed");
            }
        }
    }
}

fn is_compaction(meta: &serde_json::Value) -> bool {
    meta.get(crate::parsers::compaction::SUMMARY_META_KEY) == Some(&serde_json::Value::Bool(true))
        || [
            "/contextCompaction",
            "/jetbrains.air/contextCompaction",
            "/jetbrains/air/contextCompaction",
        ]
        .iter()
        .any(|path| meta.pointer(path).is_some_and(serde_json::Value::is_object))
}
