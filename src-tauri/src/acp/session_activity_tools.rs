use chrono::{DateTime, Utc};

use super::{ActivityContext, SessionActivity};
use crate::acp::types::AcpEvent;

struct ToolEvidence<'a> {
    started: bool,
    item_id: &'a str,
    status: Option<&'a str>,
    output: Option<&'a str>,
}

impl<'a> ToolEvidence<'a> {
    fn from_event(event: &'a AcpEvent, context: &ActivityContext<'_>) -> Option<Self> {
        let (item_id, status, raw_output, content) = match event {
            AcpEvent::ToolCall {
                tool_call_id,
                status,
                raw_output,
                content,
                ..
            } => (tool_call_id, Some(status.as_str()), raw_output, content),
            AcpEvent::ToolCallUpdate {
                tool_call_id,
                status,
                raw_output,
                content,
                ..
            } => (tool_call_id, status.as_deref(), raw_output, content),
            _ => return None,
        };
        let changed_content = content.as_deref().filter(|value| {
            context
                .tools
                .get(item_id)
                .and_then(|tool| tool.content.as_deref())
                != Some(*value)
        });
        Some(Self {
            started: matches!(event, AcpEvent::ToolCall { .. })
                && !context.tools.contains_key(item_id),
            item_id,
            status,
            output: raw_output
                .as_deref()
                .filter(|value| !value.is_empty())
                .or(changed_content),
        })
    }
}

impl SessionActivity {
    pub(super) fn observe_tool_or_boundary(
        &mut self,
        event: &AcpEvent,
        context: ActivityContext<'_>,
        now: DateTime<Utc>,
    ) -> bool {
        if let Some(evidence) = ToolEvidence::from_event(event, &context) {
            return self.note_tool(evidence, context, now);
        }
        match event {
            AcpEvent::TurnComplete { .. } => {
                self.clear_retry();
                self.urgent = true;
                true
            }
            // 用户等待结束后给运行时一个完整响应窗口。
            AcpEvent::PermissionResolved { .. }
            | AcpEvent::QuestionResolved { .. }
            | AcpEvent::ChannelConfirmationResolved { .. } => true,
            AcpEvent::UsageUpdate { .. }
            | AcpEvent::ClaudeSdkMessage { .. }
            | AcpEvent::PlanUpdate { .. }
            | AcpEvent::SessionFailure { .. }
                if context.prompting =>
            {
                true
            }
            _ => false,
        }
    }

    fn note_tool(
        &mut self,
        evidence: ToolEvidence<'_>,
        context: ActivityContext<'_>,
        now: DateTime<Utc>,
    ) -> bool {
        let has_output = evidence.output.is_some_and(|value| !value.is_empty());
        let terminal = matches!(evidence.status, Some("completed" | "failed"));
        let mut current_turn = true;
        if let Some(process) = self
            .snapshot
            .processes
            .iter_mut()
            .find(|p| p.item_id == evidence.item_id)
        {
            current_turn = process.turn_generation == context.generation;
            if has_output {
                process.output_at = Some(now);
            }
            if terminal {
                process.status = evidence.status.unwrap_or("unknown").to_string();
                process.checked_at = now;
                self.urgent = true;
            }
        }
        if !context.prompting || !current_turn {
            return false;
        }
        if evidence.started {
            self.snapshot.tool_started_at = Some(now);
        }
        if has_output {
            self.urgent |= self.snapshot.tool_output_at.is_none();
            self.snapshot.tool_output_at = Some(now);
        }
        if evidence.status.is_some() {
            self.urgent = true;
            self.clear_retry();
        }
        has_output || evidence.status.is_some()
    }
}
