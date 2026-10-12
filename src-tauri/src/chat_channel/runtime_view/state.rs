use super::types::ChannelConversationInfo;
use crate::acp::session_state::{SessionState, ToolCallStatus};
use crate::acp::types::ConnectionStatus;
use crate::db::entities::conversation::{ConversationStatus, Model};
use crate::models::AgentType;
use chrono::{DateTime, Utc};

const RECENT_OUTPUT_SECS: i64 = 10;

pub(super) fn summary(row: Model, channel_id: i32) -> ChannelConversationInfo {
    let phase = match row.status {
        ConversationStatus::Completed | ConversationStatus::PendingReview => "completed",
        ConversationStatus::Cancelled => "stopped",
        ConversationStatus::InProgress => "unknown",
    };
    ChannelConversationInfo {
        channel_id,
        conversation_id: row.id,
        folder_id: row.folder_id,
        title: row.title,
        agent_type: AgentType::from_wire(&row.agent_type).unwrap_or(AgentType::Codex),
        model: row.model,
        message_count: row.message_count,
        updated_at: row.updated_at.to_rfc3339(),
        phase: phase.into(),
        started_at: None,
        last_activity_at: None,
        current_tool: None,
        error: None,
        connection_id: None,
    }
}

pub(super) fn apply(info: &mut ChannelConversationInfo, state: &SessionState) {
    let activity = state.activity.snapshot();
    info.started_at = activity.started_at.map(|at| at.to_rfc3339());
    info.last_activity_at = [
        activity.text_at,
        activity.thinking_at,
        activity.tool_output_at,
        activity.tool_started_at,
        activity.retrying_since,
        activity.started_at,
    ]
    .into_iter()
    .flatten()
    .chain(activity.processes.iter().map(|process| process.checked_at))
    .max()
    .map(|at| at.to_rfc3339());
    info.model = state.current_model.clone().or_else(|| info.model.clone());
    info.current_tool = state
        .active_tool_calls
        .values()
        .find(|tool| {
            matches!(
                tool.status,
                ToolCallStatus::Pending | ToolCallStatus::InProgress
            )
        })
        .map(|tool| tool.label.clone());
    info.error = state.last_error.as_ref().map(|error| error.message.clone());
    info.phase = phase(state, activity.text_at).into();
}

fn phase(state: &SessionState, text_at: Option<DateTime<Utc>>) -> &'static str {
    if let Some(waiting) = waiting_phase(state) {
        return waiting;
    }
    if matches!(
        state.status,
        ConnectionStatus::Error | ConnectionStatus::Disconnected
    ) {
        return "disconnected";
    }
    if state.turn_in_flight || state.status == ConnectionStatus::Prompting {
        return running_phase(state, text_at);
    }
    if state.has_active_background_work(Utc::now()) || !state.active_delegations.is_empty() {
        return "background";
    }
    if state.last_turn_ended_abnormally {
        return if state.last_error.is_some() {
            "failed"
        } else {
            "stopped"
        };
    }
    if matches!(state.status, ConnectionStatus::Connecting) {
        return "preparing";
    }
    "idle"
}

fn waiting_phase(state: &SessionState) -> Option<&'static str> {
    if state.pending_permission.is_some() || state.pending_channel_confirmation.is_some() {
        return Some("waiting_confirmation");
    }
    if state.pending_question.is_some()
        || state
            .interactive_html
            .iter()
            .any(|page| page.wait_for_response)
    {
        return Some("waiting_input");
    }
    None
}

fn running_phase(state: &SessionState, text_at: Option<DateTime<Utc>>) -> &'static str {
    if state
        .auto_continuation
        .as_ref()
        .is_some_and(|info| matches!(info.phase.as_str(), "started" | "in_flight"))
    {
        return "recovering";
    }
    if state.activity.snapshot().retrying_since.is_some() {
        return "retrying";
    }
    if state.active_tool_calls.values().any(|tool| {
        matches!(
            tool.status,
            ToolCallStatus::Pending | ToolCallStatus::InProgress
        )
    }) {
        return "running_tool";
    }
    if text_at
        .is_some_and(|at| Utc::now().signed_duration_since(at).num_seconds() < RECENT_OUTPUT_SECS)
    {
        return "replying";
    }
    "waiting_output"
}
