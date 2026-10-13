use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::acp::session_state::{ToolCallState, ToolCallStatus};

#[derive(Debug, Clone, Copy)]
pub struct PromptStallAssessment {
    pub stalled: bool,
    pub silent_for: Duration,
    pub effective_timeout: Duration,
    pub active_tool_count: usize,
}

pub struct PromptStallInput<'a> {
    pub now: DateTime<Utc>,
    pub llm_wait_started_at: DateTime<Utc>,
    pub base_timeout: Duration,
    pub active_tool_calls: &'a BTreeMap<String, ToolCallState>,
}

pub fn assess_prompt_stall(input: PromptStallInput<'_>) -> PromptStallAssessment {
    let PromptStallInput {
        now,
        llm_wait_started_at,
        base_timeout,
        active_tool_calls,
    } = input;
    let silent_for = now
        .signed_duration_since(llm_wait_started_at)
        .to_std()
        .unwrap_or_default();
    let active_tool_count = active_tool_calls.values().filter(is_active_tool).count();

    // 工具执行及超时由工具层处理，不能据此判断 LLM 回复停滞。
    PromptStallAssessment {
        stalled: active_tool_count == 0 && silent_for >= base_timeout,
        silent_for,
        effective_timeout: base_timeout,
        active_tool_count,
    }
}

fn is_active_tool(tool: &&ToolCallState) -> bool {
    matches!(
        &tool.status,
        ToolCallStatus::Pending | ToolCallStatus::InProgress
    )
}
