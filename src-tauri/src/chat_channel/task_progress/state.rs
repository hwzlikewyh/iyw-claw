use crate::acp::session_state::SessionState;
use crate::acp::types::ConnectionStatus;
use crate::chat_channel::task_context::{self, TaskContext};

pub(super) fn phase(state: &SessionState, task: &TaskContext) -> (&'static str, usize) {
    let same_turn = state.turn_generation == task.turn_generation;
    let mut owned = task.delegations.clone();
    if same_turn {
        owned.extend(
            state
                .active_delegations
                .keys()
                .filter(|id| !task.origin.prior_delegations.contains(*id))
                .cloned(),
        );
        task_context::update(&task.id, |task| task.delegations = owned.clone());
    }
    let children = owned
        .iter()
        .filter(|id| state.active_delegations.contains_key(*id))
        .count()
        + task.background.running.len();
    if !same_turn {
        let phase = if children > 0 {
            "background"
        } else if !owned.is_empty() || task.background.seen {
            super::background::terminal_phase(task)
        } else {
            "superseded"
        };
        return (phase, children);
    }
    if !task.background.running.is_empty() && state.background_uncertain {
        return ("unknown", children);
    }
    let phase = current_phase(state, children);
    if matches!(phase, "completed" | "failed" | "stopped") && task.tools_active {
        return ("settling", children);
    }
    (
        if phase == "completed" {
            super::background::terminal_phase(task)
        } else {
            phase
        },
        children,
    )
}

fn current_phase(state: &SessionState, children: usize) -> &'static str {
    if state.pending_question.is_some()
        || state.pending_permission.is_some()
        || state.pending_channel_confirmation.is_some()
    {
        return "waiting_input";
    }
    if matches!(
        state.status,
        ConnectionStatus::Disconnected | ConnectionStatus::Error
    ) {
        return "unknown";
    }
    if state.turn_in_flight || state.turn_completion_pending {
        return "running";
    }
    if children > 0 {
        return "background";
    }
    if state.last_turn_ended_abnormally {
        return if state.last_error.is_some() {
            "failed"
        } else {
            "stopped"
        };
    }
    "completed"
}
