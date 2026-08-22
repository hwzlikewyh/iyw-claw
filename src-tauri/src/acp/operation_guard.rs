use std::sync::atomic::Ordering;

use crate::acp::session_state::SessionState;
use crate::acp::types::ConnectionStatus;

pub(crate) fn active_operation_reason(
    state: &SessionState,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<&'static str> {
    if !matches!(
        state.status,
        ConnectionStatus::Connected | ConnectionStatus::Disconnected | ConnectionStatus::Error
    ) || state.turn_in_flight
        || state.turn_completion_pending
    {
        return Some("turn_in_progress");
    }
    if state.pending_permission.is_some() || state.pending_question.is_some() {
        return Some("interaction_pending");
    }
    if state.pending_channel_confirmation.is_some()
        || state.agent_inputs.iter().any(|item| !item.is_terminal())
    {
        return Some("input_pending");
    }
    if state.native_background_turn.is_some()
        || !state.active_tool_calls.is_empty()
        || !state.active_delegations.is_empty()
        || state.has_active_background_work(now)
        || state.active_terminal_count.load(Ordering::Acquire) > 0
    {
        return Some("background_work_active");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentType;

    fn state(status: ConnectionStatus) -> SessionState {
        let mut state = SessionState::new(
            "test-connection".to_string(),
            AgentType::Codex,
            None,
            "test-owner".to_string(),
            None,
        );
        state.status = status;
        state
    }

    #[test]
    fn idle_connected_session_is_replaceable() {
        assert_eq!(
            active_operation_reason(&state(ConnectionStatus::Connected), chrono::Utc::now()),
            None
        );
    }

    #[test]
    fn prompt_and_terminal_activity_block_replacement() {
        let mut prompting = state(ConnectionStatus::Connected);
        prompting.turn_in_flight = true;
        assert_eq!(
            active_operation_reason(&prompting, chrono::Utc::now()),
            Some("turn_in_progress")
        );

        let terminal = state(ConnectionStatus::Connected);
        terminal
            .active_terminal_count
            .fetch_add(1, Ordering::Release);
        assert_eq!(
            active_operation_reason(&terminal, chrono::Utc::now()),
            Some("background_work_active")
        );
    }

    #[test]
    fn startup_state_blocks_replacement() {
        assert_eq!(
            active_operation_reason(&state(ConnectionStatus::Connecting), chrono::Utc::now()),
            Some("turn_in_progress")
        );
    }
}
