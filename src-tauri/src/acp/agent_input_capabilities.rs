use crate::acp::{AgentInputPayload, PromptInputBlock};
use crate::models::AgentType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentInputBlockKind {
    Text,
    Image,
    Resource,
    ResourceLink,
}

/// Native protocols documented by upstream Agents. A variant may only be
/// returned by `for_connection` after its adapter exposes a consumption ack.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSteerKind {
    CodexTurnSteer,
    OpenCodePromptAsync,
    OpenClawQueueSteer,
    HermesSteer,
    KimiSessionSteer,
    ClineDeliverySteer,
    CodeBuddyBusyQueue,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentInputCapabilities {
    native_steer_kind: Option<NativeSteerKind>,
    accepted_block_kinds: &'static [AgentInputBlockKind],
    has_consumption_ack: bool,
    supports_cooperative_feedback: bool,
    deferred_interrupt: bool,
}

impl AgentInputCapabilities {
    pub(crate) fn for_connection(agent_type: AgentType, feedback_tool_available: bool) -> Self {
        let deferred_interrupt = matches!(
            agent_type,
            AgentType::ClaudeCode | AgentType::Gemini | AgentType::Pi | AgentType::Grok
        );
        Self {
            // No current generic ACP adapter provides a reliable native-steer
            // consumption ack. Keep this explicit instead of treating a second
            // session/prompt as steer.
            native_steer_kind: None,
            accepted_block_kinds: &[],
            has_consumption_ack: false,
            supports_cooperative_feedback: feedback_tool_available && !deferred_interrupt,
            deferred_interrupt,
        }
    }

    pub(crate) fn native_steer_for(&self, payload: &AgentInputPayload) -> Option<NativeSteerKind> {
        let kind = self.native_steer_kind?;
        if !self.has_consumption_ack
            || !payload
                .blocks
                .iter()
                .all(|block| self.accepted_block_kinds.contains(&block_kind(block)))
        {
            return None;
        }
        Some(kind)
    }

    pub(crate) fn supports_feedback(&self, payload: &AgentInputPayload) -> bool {
        let text_only = payload
            .blocks
            .iter()
            .all(|block| matches!(block, PromptInputBlock::Text { .. }));
        let has_text = payload.blocks.iter().any(
            |block| matches!(block, PromptInputBlock::Text { text } if !text.trim().is_empty()),
        );
        let text_matches_display =
            feedback_text(payload).is_some_and(|text| text == payload.display_text.trim());
        self.supports_cooperative_feedback
            && payload.mode_id.is_none()
            && text_only
            && has_text
            && text_matches_display
    }

    pub(crate) const fn uses_deferred_interrupt(&self) -> bool {
        self.deferred_interrupt
    }
}

fn block_kind(block: &PromptInputBlock) -> AgentInputBlockKind {
    match block {
        PromptInputBlock::Text { .. } => AgentInputBlockKind::Text,
        PromptInputBlock::Image { .. } => AgentInputBlockKind::Image,
        PromptInputBlock::Resource { .. } => AgentInputBlockKind::Resource,
        PromptInputBlock::ResourceLink { .. } => AgentInputBlockKind::ResourceLink,
    }
}

pub(crate) fn feedback_text(payload: &AgentInputPayload) -> Option<String> {
    let mut parts = Vec::new();
    for block in &payload.blocks {
        match block {
            PromptInputBlock::Text { text } if !text.trim().is_empty() => {
                parts.push(text.trim());
            }
            PromptInputBlock::Text { .. } => {}
            _ => return None,
        }
    }
    let text = parts.join("\n");
    (!text.is_empty()).then_some(text)
}
