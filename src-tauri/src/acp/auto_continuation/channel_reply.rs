use super::{has_blocking_work, AutoContinuationEvidence};
use crate::acp::session_state::{LiveContentBlock, SessionState};

pub(super) fn evaluate(state: &SessionState, reason: &str) -> Option<AutoContinuationEvidence> {
    // 渠道追问不一定广播 UserMessage，使用已接收回合的并发门禁。
    if !matches!(reason, "empty" | "end_turn")
        || !state.owner_window_label.starts_with("chat_channel:")
        || state.conversation_id.is_none()
        || !state.turn_in_flight
        || state.is_delegation_child
        || has_blocking_work(state)
        || state
            .interactive_html
            .iter()
            .any(|page| page.wait_for_response)
        || state
            .live_message
            .iter()
            .flat_map(|message| &message.content)
            .any(
                |block| matches!(block, LiveContentBlock::Text { text } if !text.trim().is_empty()),
            )
    {
        return None;
    }
    Some(AutoContinuationEvidence {
        reason_code: "channel_empty_reply",
        evidence_kind: "empty_reply",
        auto_run: true,
    })
}
