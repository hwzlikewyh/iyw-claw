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
            self.invalidate();
            return true;
        };
        if self.confirmed_session.as_deref() == Some(session) {
            return false;
        }
        self.confirmed_session = None;
        self.pending = Some((generation, session.to_owned()));
        true
    }

    pub(crate) fn invalidate(&mut self) {
        self.confirmed_session = None;
        self.pending = None;
    }

    pub(crate) fn observe(&mut self, event: &AcpEvent, generation: i64) {
        match event {
            AcpEvent::SessionStarted { .. } | AcpEvent::ContentRecovered { .. } => {
                self.invalidate();
            }
            AcpEvent::UserMessage { blocks, .. }
                if blocks.iter().any(|block| {
                    matches!(block,
                UserMessageBlock::Text { text } if text.trim_start().starts_with('/'))
                }) =>
            {
                self.invalidate();
            }
            AcpEvent::StatusChanged {
                status: ConnectionStatus::Disconnected | ConnectionStatus::Error,
            }
            | AcpEvent::Error { terminal: true, .. } => self.invalidate(),
            AcpEvent::ToolCall { meta, .. } | AcpEvent::ToolCallUpdate { meta, .. } => {
                if meta.as_ref().is_some_and(is_compaction) {
                    self.invalidate();
                }
            }
            AcpEvent::TurnComplete {
                session_id,
                stop_reason,
                ..
            } => {
                if stop_reason != "end_turn" {
                    self.invalidate();
                } else if let Some((pending_generation, session)) = self.pending.take() {
                    if pending_generation == generation && session == *session_id {
                        self.confirmed_session = Some(session);
                    }
                }
            }
            _ => {}
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
