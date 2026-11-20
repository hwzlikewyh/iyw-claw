//! Shared retry and transport fallback decisions for Responses requests.
//! Content-filter guidance is recorded for sampling requests before retry decisions.
//! Server advice controls timing without extending configured retry limits.

use std::time::Duration;

use crate::client::ModelClientSession;
use crate::context::ContentFilterGuidance;
use crate::context::ContextualUserFragment;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::session::turn_context::TurnContext;
use codex_client::RetryOperation;
use codex_features::Feature;
use codex_http_client::RetryAfter;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::WarningEvent;
use tokio::time::Instant;
use tracing::warn;

const INITIAL_CONNECTION_RETRY_DELAY: Duration = Duration::from_secs(10);
const MAX_CONNECTION_RETRY_DELAY: Duration = Duration::from_secs(120);
const MANAGED_GATEWAY_PROVIDER: &str = "iyw-claw";
pub(crate) const MANAGED_GATEWAY_MAX_RETRIES: u64 = 3;

pub(crate) fn is_request_too_large(err: &CodexErr) -> bool {
    matches!(err.details(), CodexErrorDetails::UnexpectedStatus(response)
        if response.status == http::StatusCode::PAYLOAD_TOO_LARGE)
}

pub(crate) fn is_retryable_response_error(provider_name: &str, err: &CodexErr) -> bool {
    if is_managed_gateway_timeout(provider_name, err) {
        return false;
    }
    if is_output_limit_stream_error(err) {
        return false;
    }
    if let CodexErrorDetails::UnexpectedStatus(response) = err.details() {
        if response.status.is_client_error() {
            return matches!(
                response.status,
                http::StatusCode::REQUEST_TIMEOUT
                    | http::StatusCode::CONFLICT
                    | http::StatusCode::TOO_MANY_REQUESTS
            );
        }
    }
    err.retry_delay(1).is_some()
}

fn is_output_limit_stream_error(err: &CodexErr) -> bool {
    match err.details() {
        CodexErrorDetails::ResponseIncomplete(error) => error.reason == "max_output_tokens",
        CodexErrorDetails::Stream(detail) => {
            detail
                .strip_prefix("Incomplete response returned, reason:")
                .and_then(|reason| reason.split_whitespace().next())
                == Some("max_output_tokens")
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ResponsesStreamRequest {
    Sampling,
    RemoteCompactionV2,
}

/// A new generation cannot repair a terminal provider response or settle the
/// previous attempt's bill. Preserve these outcomes across transport retries,
/// same-turn steering, and compaction's fallback to another model.
pub(crate) fn is_terminal_sampling_outcome(error: &CodexErr) -> bool {
    matches!(
        error.details(),
        CodexErrorDetails::ResponseIncomplete(_)
            | CodexErrorDetails::UsageAccountingFailed(_)
            | CodexErrorDetails::SessionBudgetExceeded
            | CodexErrorDetails::InferenceHookDenied { .. }
    )
}

#[derive(Default)]
pub(crate) struct ResponsesStreamRetryState {
    retries: u64,
    connection_retries: u64,
}

fn connection_backoff(attempt: u64) -> Duration {
    codex_client::backoff(
        INITIAL_CONNECTION_RETRY_DELAY,
        attempt.clamp(1, u64::from(u32::MAX)),
    )
    .min(MAX_CONNECTION_RETRY_DELAY)
}

pub(crate) fn model_retry_delay(err: &CodexErr, attempt: u64) -> Option<Duration> {
    let delay = err.retry_delay(attempt)?;
    if err.retry_after().is_some() {
        return Some(delay);
    }
    match err.details() {
        CodexErrorDetails::ConnectionFailed(_)
        | CodexErrorDetails::ResponseStreamFailed(_)
        | CodexErrorDetails::Stream(_)
        | CodexErrorDetails::UnexpectedStatus(_)
        | CodexErrorDetails::RateLimitExceeded(_)
        | CodexErrorDetails::InternalServerError
        | CodexErrorDetails::Io(_)
        | CodexErrorDetails::RequestTimeout
        | CodexErrorDetails::Timeout => Some(connection_backoff(attempt)),
        _ => Some(delay),
    }
}

/// Server retry advice retained after stream retries are exhausted. The turn ID
/// prevents a reused Guardian session from applying advice from an earlier review.
pub(crate) struct ExhaustedResponseRetry {
    pub(crate) turn_id: String,
    pub(crate) retry_at: Option<tokio::time::Instant>,
}

/// Returns `Ok(())` when the caller should retry the request loop, or the original error when
/// it is terminal or the retry budget is exhausted.
#[tracing::instrument(level = "trace", skip_all, fields(request = ?request))]
pub(crate) async fn handle_response_stream_error(
    retry_state: &mut ResponsesStreamRetryState,
    max_retries: u64,
    err: CodexErr,
    client_session: &mut ModelClientSession,
    sess: &Session,
    step_context: &StepContext,
    request: ResponsesStreamRequest,
) -> Result<(), CodexErr> {
    let turn_context = &step_context.turn;
    let provider_name = turn_context.provider.info().name.as_str();
    let managed_gateway = provider_name == MANAGED_GATEWAY_PROVIDER;
    let managed_gateway_timeout = is_managed_gateway_timeout(provider_name, &err);
    let max_retries = if managed_gateway {
        max_retries.min(MANAGED_GATEWAY_MAX_RETRIES)
    } else {
        max_retries
    };
    let retries_for_limit = retry_state.retries;
    if managed_gateway_timeout {
        warn!(
            turn_id = %turn_context.sub_id,
            provider = %provider_name,
            retry_count = retries_for_limit,
            max_retries,
            action = if retries_for_limit < max_retries { "retry" } else { "stop" },
            "managed gateway returned 504; applying bounded request retry"
        );
    }
    // 请求体不变时重连无法消除 413，交由回合层进行有界压缩恢复。
    if is_request_too_large(&err) {
        warn!(
            turn_id = %turn_context.sub_id,
            ?request,
            "request body rejected with HTTP 413; skipping unchanged retry"
        );
        return Err(err);
    }
    // 余额、认证及参数错误不会因重连或切换传输而恢复。
    if !managed_gateway_timeout && !is_retryable_response_error(provider_name, &err) {
        return Err(err);
    }
    if matches!(request, ResponsesStreamRequest::Sampling)
        && matches!(err.details(), CodexErrorDetails::ContentFilter)
    {
        let model_info = &step_context.settings.model_info;
        let guidance = ContentFilterGuidance {
            text: codex_prompts::ResolvedModelMessages::from_model(model_info)
                .content_filter_guidance()
                .to_string(),
        };
        sess.record_conversation_items(
            turn_context,
            model_info,
            &[ContextualUserFragment::into(guidance)],
        )
        .await;
    }
    let operation = match request {
        ResponsesStreamRequest::Sampling => RetryOperation::Sampling,
        ResponsesStreamRequest::RemoteCompactionV2 => RetryOperation::RemoteCompactionV2,
    };
    let retry_count = retry_state.retries.saturating_add(1);
    let retry_number = retries_for_limit.saturating_add(1);
    let delay_attempt = if managed_gateway_timeout {
        retry_number
    } else {
        retry_count
    };
    let Some(delay) = model_retry_delay(&err, delay_attempt) else {
        return Err(err);
    };
    let retry_after = err.retry_after();

    if turn_context
        .config
        .features
        .enabled(Feature::UnboundedConnectionRetries)
        && !managed_gateway
        && matches!(request, ResponsesStreamRequest::Sampling)
        && matches!(err.details(), CodexErrorDetails::ConnectionFailed(_))
        && !turn_context.session_source.is_internal()
        && !turn_context.provider.info().is_amazon_bedrock()
    {
        let retry_delay = err.server_retry_delay().unwrap_or_else(|| {
            connection_backoff(retry_state.connection_retries.saturating_add(1))
        });
        warn!(
            turn_id = %turn_context.sub_id,
            error = %err,
            ?retry_delay,
            "stream connection failed; waiting to retry"
        );
        sess.notify_stream_error(turn_context, "Reconnecting... waiting for network", err)
            .await;
        retry_state.connection_retries = retry_state.connection_retries.saturating_add(1);
        codex_client::record_retry!(retry_state.connection_retries, retry_delay, operation);
        tokio::time::sleep(retry_delay).await;
        return Ok(());
    }

    let requires_http_fallback = !managed_gateway && err.requires_http_fallback();
    let switched = !managed_gateway && (requires_http_fallback || retry_state.retries >= max_retries)
        && client_session.try_switch_fallback_transport(
            &turn_context.session_telemetry,
            turn_context.model_info(),
        );
    if switched {
        // Changing transport must not bypass the server's retry deadline.
        if let Some(retry_after) = retry_after {
            tokio::time::sleep_until(retry_after.deadline()).await;
        }
        sess.send_event(
            turn_context,
            EventMsg::Warning(WarningEvent {
                message: format!("Falling back from WebSockets to HTTPS transport. {err:#}"),
            }),
        )
        .await;
    }
    // Another request can already have switched this session to HTTP.
    if switched || requires_http_fallback {
        retry_state.retries = 0;
        return Ok(());
    }

    if retries_for_limit < max_retries {
        retry_state.retries = retry_count;
        log_retry(
            request,
            turn_context,
            &err,
            retry_number,
            max_retries,
            delay,
        );

        // In release builds, hide the first websocket retry notification to reduce noisy
        // transient reconnect messages. In debug builds, keep full visibility for diagnosis.
        let report_error = retry_count > 1
            || cfg!(debug_assertions)
            || !sess.services.model_client.responses_websocket_enabled();
        if report_error {
            // Surface retry information to any UI/front-end so the user understands what is
            // happening instead of staring at a seemingly frozen screen.
            sess.notify_stream_error(
                turn_context,
                format!("Reconnecting... {retry_number}/{max_retries}"),
                err,
            )
            .await;
        }
        // Use one clock sample so local backoff telemetry retains the selected delay.
        let now = Instant::now();
        let retry_at = retry_after.map(RetryAfter::deadline).unwrap_or(now + delay);
        let delay = retry_at.saturating_duration_since(now);
        codex_client::record_retry!(retry_number, delay, operation);
        tokio::time::sleep_until(retry_at).await;
        return Ok(());
    }

    sess.services
        .thread_extension_data
        .insert(ExhaustedResponseRetry {
            turn_id: turn_context.sub_id.clone(),
            retry_at: retry_after.map(RetryAfter::deadline),
        });
    Err(err)
}

pub(crate) fn is_managed_gateway_timeout(provider_name: &str, err: &CodexErr) -> bool {
    provider_name == MANAGED_GATEWAY_PROVIDER
        && matches!(err.details(), CodexErrorDetails::UnexpectedStatus(response)
            if response.status == http::StatusCode::GATEWAY_TIMEOUT)
}

fn log_retry(
    request: ResponsesStreamRequest,
    turn_context: &TurnContext,
    err: &CodexErr,
    retries: u64,
    max_retries: u64,
    delay: Duration,
) {
    match request {
        ResponsesStreamRequest::Sampling => {
            warn!(
                turn_id = %turn_context.sub_id,
                retries,
                max_retries,
                sampling_error = %err,
                "stream disconnected - retrying sampling request ({retries}/{max_retries} in {delay:?})...",
            );
        }
        ResponsesStreamRequest::RemoteCompactionV2 => {
            warn!(
                turn_id = %turn_context.sub_id,
                retries,
                max_retries,
                compact_error = %err,
                "remote compaction v2 stream failed; retrying request after delay"
            );
        }
    }
}
