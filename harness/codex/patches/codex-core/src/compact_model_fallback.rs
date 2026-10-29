use crate::session::turn_context::TurnContext;
use codex_analytics::CompactionImplementation;
use codex_analytics::CompactionReason;
use codex_otel::SessionTelemetry;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use tracing::warn;

/// Returns whether a failed compaction attempt should use the current model.
pub(crate) fn should_retry_with_current_model(
    error: &CodexErr,
    previous: &TurnContext,
    current: &TurnContext,
) -> bool {
    may_use_current_model(error, previous, current)
        && current.provider.info().is_openai()
        && current
            .auth_manager
            .as_deref()
            .and_then(codex_login::AuthManager::auth_cached)
            .is_some_and(|auth| auth.uses_codex_backend() || auth.is_api_key_auth())
}

/// 本地摘要沿用当前提供方，不引入其它模型或绕过配额、取消和参数错误。
pub(crate) fn should_retry_responses_with_current_model(
    error: &CodexErr,
    previous: &TurnContext,
    current: &TurnContext,
) -> bool {
    may_use_current_model(error, previous, current)
        && !matches!(
            error.details(),
            CodexErrorDetails::RateLimitExceeded(_) | CodexErrorDetails::ContentFilter
        )
        && !matches!(error.details(), CodexErrorDetails::UnexpectedStatus(response)
            if response.status == http::StatusCode::TOO_MANY_REQUESTS)
        && (crate::responses_retry::is_retryable_response_error(error)
            || matches!(
                error.details(),
                CodexErrorDetails::ModelNotFound { .. } | CodexErrorDetails::ContextWindowExceeded
            ))
}

fn may_use_current_model(error: &CodexErr, previous: &TurnContext, current: &TurnContext) -> bool {
    if matches!(error.details(), CodexErrorDetails::UnexpectedStatus(response)
        if matches!(response.status, http::StatusCode::UNAUTHORIZED
            | http::StatusCode::PAYMENT_REQUIRED | http::StatusCode::FORBIDDEN))
    {
        return false;
    }
    if matches!(
        error.details(),
        CodexErrorDetails::TurnAborted
            | CodexErrorDetails::Interrupted
            | CodexErrorDetails::SessionBudgetExceeded
            | CodexErrorDetails::ResponseIncomplete(_)
            | CodexErrorDetails::RefreshTokenFailed(_)
    ) || (previous.model_info().slug == current.model_info().slug
        && previous.cyber_access_program == current.cyber_access_program)
    {
        return false;
    }

    true
}

pub(crate) fn record_model_fallback(
    session_telemetry: &SessionTelemetry,
    previous_model: &str,
    current_model: &str,
    reason: CompactionReason,
    implementation: CompactionImplementation,
    fallback_error: Option<&CodexErr>,
) {
    let reason_tag = match reason {
        CompactionReason::UserRequested => "user_requested",
        CompactionReason::ContextLimit => "context_limit",
        CompactionReason::ModelDownshift => "model_downshift",
        CompactionReason::CompHashChanged => "comp_hash_changed",
    };
    let implementation_tag = match implementation {
        CompactionImplementation::Responses => "responses",
        CompactionImplementation::ResponsesCompactionV2 => "responses_compaction_v2",
    };
    let outcome = if fallback_error.is_none() {
        "succeeded"
    } else {
        "failed"
    };
    session_telemetry.counter(
        "codex.compaction.model_fallback",
        /*inc*/ 1,
        &[
            ("reason", reason_tag),
            ("implementation", implementation_tag),
            ("outcome", outcome),
        ],
    );
    warn!(
        previous_model,
        current_model,
        ?reason,
        ?implementation,
        outcome,
        ?fallback_error,
        "previous-model compaction failed; retried with current model"
    );
}
