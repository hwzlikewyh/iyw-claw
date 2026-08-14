use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::acp::session_state::{ToolCallState, ToolCallStatus};

pub const TOOL_TIMEOUT_GRACE: Duration = Duration::from_secs(60);
pub const FALLBACK_TOOL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const MAX_TOOL_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy)]
pub enum PromptStallTimeoutSource {
    Base,
    Declared,
    Fallback,
}

impl PromptStallTimeoutSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Declared => "declared",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PromptStallAssessment {
    pub stalled: bool,
    pub silent_for: Duration,
    pub effective_timeout: Duration,
    pub active_tool_count: usize,
    pub longest_tool_runtime: Duration,
    pub timeout_source: PromptStallTimeoutSource,
}

pub struct PromptStallInput<'a> {
    pub now: DateTime<Utc>,
    pub monotonic_now: Instant,
    pub last_agent_event_at: DateTime<Utc>,
    pub base_timeout: Duration,
    pub active_tool_calls: &'a BTreeMap<String, ToolCallState>,
}

struct ToolStallSummary {
    remaining: Duration,
    longest_runtime: Duration,
    active_count: usize,
    all_stalled: bool,
    source: PromptStallTimeoutSource,
}

pub fn assess_prompt_stall(input: PromptStallInput<'_>) -> PromptStallAssessment {
    let PromptStallInput {
        now,
        monotonic_now,
        last_agent_event_at,
        base_timeout,
        active_tool_calls,
    } = input;
    let silent_for = now
        .signed_duration_since(last_agent_event_at)
        .to_std()
        .unwrap_or_default();
    let base_remaining = base_timeout.saturating_sub(silent_for);
    let tools = summarize_active_tools(monotonic_now, active_tool_calls);
    let effective_remaining = base_remaining.max(tools.remaining);
    let timeout_source = if tools.remaining > base_remaining {
        tools.source
    } else {
        PromptStallTimeoutSource::Base
    };

    PromptStallAssessment {
        stalled: silent_for >= base_timeout && tools.all_stalled,
        silent_for,
        effective_timeout: silent_for.saturating_add(effective_remaining),
        active_tool_count: tools.active_count,
        longest_tool_runtime: tools.longest_runtime,
        timeout_source,
    }
}

fn summarize_active_tools(
    monotonic_now: Instant,
    active_tool_calls: &BTreeMap<String, ToolCallState>,
) -> ToolStallSummary {
    let mut tool_remaining = Duration::ZERO;
    let mut longest_tool_runtime = Duration::ZERO;
    let mut active_tool_count = 0;
    let mut all_tools_stalled = true;
    let mut tool_source = PromptStallTimeoutSource::Fallback;
    for tool in active_tool_calls.values().filter(is_active_tool) {
        active_tool_count += 1;
        let runtime = tool
            .started_at
            .map(|started_at| monotonic_now.saturating_duration_since(started_at))
            .unwrap_or_default();
        let (window, source) = tool_timeout_window(tool);
        let remaining = window.saturating_sub(runtime);
        if remaining > tool_remaining {
            tool_remaining = remaining;
            tool_source = source;
        }
        longest_tool_runtime = longest_tool_runtime.max(runtime);
        all_tools_stalled &= runtime >= window;
    }
    ToolStallSummary {
        remaining: tool_remaining,
        longest_runtime: longest_tool_runtime,
        active_count: active_tool_count,
        all_stalled: all_tools_stalled,
        source: tool_source,
    }
}

fn is_active_tool(tool: &&ToolCallState) -> bool {
    matches!(
        &tool.status,
        ToolCallStatus::Pending | ToolCallStatus::InProgress
    )
}

fn tool_timeout_window(tool: &ToolCallState) -> (Duration, PromptStallTimeoutSource) {
    let Some(timeout_ms) = tool
        .input
        .as_ref()
        .and_then(|input| input.get("timeout_ms"))
        .and_then(serde_json::Value::as_u64)
        .filter(|timeout_ms| *timeout_ms > 0)
    else {
        return (FALLBACK_TOOL_TIMEOUT, PromptStallTimeoutSource::Fallback);
    };

    let timeout_secs = timeout_ms
        .saturating_add(999)
        .checked_div(1000)
        .unwrap_or(u64::MAX)
        .saturating_add(TOOL_TIMEOUT_GRACE.as_secs())
        .min(MAX_TOOL_TIMEOUT.as_secs());
    (
        Duration::from_secs(timeout_secs),
        PromptStallTimeoutSource::Declared,
    )
}
