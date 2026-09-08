use std::time::Duration;

use codex_analytics::GuardianApprovalRequestSource;
use codex_analytics::GuardianReviewAnalyticsResult;
use codex_analytics::GuardianReviewDecision;
use codex_analytics::GuardianReviewFailureReason;
use codex_analytics::GuardianReviewSessionKind;
use codex_analytics::GuardianReviewTerminalStatus;
use codex_analytics::GuardianReviewedAction;
use codex_mcp::CODEX_APPS_MCP_SERVER_NAME;
use codex_otel::GUARDIAN_REVIEW_COUNT_METRIC;
use codex_otel::GUARDIAN_REVIEW_DURATION_METRIC;
use codex_otel::GUARDIAN_REVIEW_TOKEN_USAGE_METRIC;
use codex_otel::GUARDIAN_REVIEW_TTFT_DURATION_METRIC;
use codex_otel::SessionTelemetry;
use codex_otel::sanitize_metric_tag_value;
use codex_protocol::protocol::GuardianAssessmentOutcome;
use codex_protocol::protocol::GuardianRiskLevel;
use codex_protocol::protocol::GuardianUserAuthorization;
use codex_protocol::protocol::TokenUsage;

pub(crate) fn emit_guardian_review_metrics(
    session_telemetry: &SessionTelemetry,
    result: &GuardianReviewAnalyticsResult,
    approval_request_source: GuardianApprovalRequestSource,
    reviewed_action: &GuardianReviewedAction,
    completion_latency_ms: u64,
) {
    let tags = guardian_review_metric_tags(result, approval_request_source, reviewed_action);
    let tag_refs: Vec<(&str, &str)> = tags
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();

    // Custom MCP tool names may contain user data; only attribute tools from the OpenAI Apps server.
    let mcp_tags = if let GuardianReviewedAction::McpToolCall {
        server, tool_name, ..
    } = reviewed_action
        && server == CODEX_APPS_MCP_SERVER_NAME
    {
        vec![("tool", sanitize_metric_tag_value(tool_name))]
    } else {
        Vec::new()
    };
    let mut counter_tag_refs = tag_refs.clone();
    counter_tag_refs.extend(mcp_tags.iter().map(|(key, value)| (*key, value.as_str())));
    session_telemetry.counter(
        GUARDIAN_REVIEW_COUNT_METRIC,
        /*inc*/ 1,
        &counter_tag_refs,
    );
    session_telemetry.record_duration(
        GUARDIAN_REVIEW_DURATION_METRIC,
        Duration::from_millis(completion_latency_ms),
        &tag_refs,
    );

    if let Some(time_to_first_token_ms) = result.time_to_first_token_ms {
        session_telemetry.record_duration(
            GUARDIAN_REVIEW_TTFT_DURATION_METRIC,
            Duration::from_millis(time_to_first_token_ms),
            &tag_refs,
        );
    }

    if let Some(token_usage) = result.token_usage.as_ref() {
        emit_guardian_token_usage_histograms(session_telemetry, token_usage, tags);
    }
}

fn emit_guardian_token_usage_histograms(
    session_telemetry: &SessionTelemetry,
    token_usage: &TokenUsage,
    base_tags: Vec<(&'static str, String)>,
) {
    for (token_type, value) in [
        ("total", token_usage.total_tokens.max(0)),
        ("input", token_usage.input_tokens.max(0)),
        ("cached_input", token_usage.cached_input()),
        (
            "cache_write_input",
            token_usage.cache_write_input_tokens.max(0),
        ),
        ("non_cached_input", token_usage.non_cached_input()),
        ("output", token_usage.output_tokens.max(0)),
        (
            "reasoning_output",
            token_usage.reasoning_output_tokens.max(0),
        ),
    ] {
        let mut tags = base_tags.clone();
        tags.push(("token_type", token_type.to_string()));
        let tag_refs: Vec<(&str, &str)> = tags
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        session_telemetry.histogram(GUARDIAN_REVIEW_TOKEN_USAGE_METRIC, value, &tag_refs);
    }
}

fn guardian_review_metric_tags(
    result: &GuardianReviewAnalyticsResult,
    approval_request_source: GuardianApprovalRequestSource,
    reviewed_action: &GuardianReviewedAction,
) -> Vec<(&'static str, String)> {
    vec![
        ("decision", decision_tag(result.decision).to_string()),
        (
            "terminal_status",
            terminal_status_tag(result.terminal_status).to_string(),
        ),
        (
            "failure_reason",
            failure_reason_tag(result.failure_reason).to_string(),
        ),
        (
            "approval_request_source",
            approval_request_source_tag(approval_request_source).to_string(),
        ),
        ("action", reviewed_action_tag(reviewed_action).to_string()),
        (
            "session_kind",
            session_kind_tag(result.guardian_session_kind).to_string(),
        ),
        (
            "had_prior_review_context",
            optional_bool_tag(result.had_prior_review_context).to_string(),
        ),
        (
            "reviewed_action_truncated",
            bool_tag(result.reviewed_action_truncated).to_string(),
        ),
        ("risk_level", risk_level_tag(result.risk_level).to_string()),
        (
            "user_authorization",
            user_authorization_tag(result.user_authorization).to_string(),
        ),
        ("outcome", outcome_tag(result.outcome).to_string()),
        (
            "guardian_model",
            result
                .guardian_model
                .as_deref()
                .map(sanitize_metric_tag_value)
                .unwrap_or_else(|| "none".to_string()),
        ),
        (
            "guardian_reasoning_effort",
            result
                .guardian_reasoning_effort
                .as_deref()
                .map(sanitize_metric_tag_value)
                .unwrap_or_else(|| "none".to_string()),
        ),
    ]
}

fn decision_tag(decision: GuardianReviewDecision) -> &'static str {
    match decision {
        GuardianReviewDecision::Approved => "approved",
        GuardianReviewDecision::Denied => "denied",
        GuardianReviewDecision::Aborted => "aborted",
    }
}

fn terminal_status_tag(status: GuardianReviewTerminalStatus) -> &'static str {
    match status {
        GuardianReviewTerminalStatus::Approved => "approved",
        GuardianReviewTerminalStatus::Denied => "denied",
        GuardianReviewTerminalStatus::Aborted => "aborted",
        GuardianReviewTerminalStatus::TimedOut => "timed_out",
        GuardianReviewTerminalStatus::FailedClosed => "failed_closed",
    }
}

fn failure_reason_tag(reason: Option<GuardianReviewFailureReason>) -> &'static str {
    match reason {
        Some(GuardianReviewFailureReason::Timeout) => "timeout",
        Some(GuardianReviewFailureReason::Cancelled) => "cancelled",
        Some(GuardianReviewFailureReason::PromptBuildError) => "prompt_build_error",
        Some(GuardianReviewFailureReason::SessionError) => "session_error",
        Some(GuardianReviewFailureReason::ParseError) => "parse_error",
        None => "none",
    }
}

fn approval_request_source_tag(source: GuardianApprovalRequestSource) -> &'static str {
    match source {
        GuardianApprovalRequestSource::MainTurn => "main_turn",
        GuardianApprovalRequestSource::DelegatedSubagent => "delegated_subagent",
    }
}

fn reviewed_action_tag(action: &GuardianReviewedAction) -> &'static str {
    match action {
        GuardianReviewedAction::Shell { .. } => "shell",
        GuardianReviewedAction::UnifiedExec { .. } => "unified_exec",
        GuardianReviewedAction::WriteStdin { .. } => "write_stdin",
        GuardianReviewedAction::Execve { .. } => "execve",
        GuardianReviewedAction::ApplyPatch {} => "apply_patch",
        GuardianReviewedAction::NetworkAccess { .. } => "network_access",
        GuardianReviewedAction::McpToolCall { .. } => "mcp_tool_call",
        GuardianReviewedAction::RequestPermissions {} => "request_permissions",
    }
}

fn session_kind_tag(kind: Option<GuardianReviewSessionKind>) -> &'static str {
    match kind {
        Some(GuardianReviewSessionKind::TrunkNew) => "trunk_new",
        Some(GuardianReviewSessionKind::TrunkReused) => "trunk_reused",
        Some(GuardianReviewSessionKind::EphemeralForked) => "ephemeral_forked",
        None => "none",
    }
}

fn optional_bool_tag(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

fn bool_tag(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn risk_level_tag(risk_level: Option<GuardianRiskLevel>) -> &'static str {
    match risk_level {
        Some(GuardianRiskLevel::Low) => "low",
        Some(GuardianRiskLevel::Medium) => "medium",
        Some(GuardianRiskLevel::High) => "high",
        Some(GuardianRiskLevel::Critical) => "critical",
        None => "none",
    }
}

fn user_authorization_tag(user_authorization: Option<GuardianUserAuthorization>) -> &'static str {
    match user_authorization {
        Some(GuardianUserAuthorization::Unknown) => "unknown",
        Some(GuardianUserAuthorization::Low) => "low",
        Some(GuardianUserAuthorization::Medium) => "medium",
        Some(GuardianUserAuthorization::High) => "high",
        None => "none",
    }
}

fn outcome_tag(outcome: Option<GuardianAssessmentOutcome>) -> &'static str {
    match outcome {
        Some(GuardianAssessmentOutcome::Allow) => "allow",
        Some(GuardianAssessmentOutcome::Deny) => "deny",
        None => "none",
    }
}
