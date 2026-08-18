use super::Envelope;
use crate::app_error::{AppCommandError, AppErrorCode};

const DISTRIBUTION_NOT_FOUND: &str = "AGENT_DISTRIBUTION_NOT_FOUND";
const ARTIFACT_NOT_READY: &str = "AGENT_ARTIFACT_NOT_READY";

pub(super) fn envelope_error(envelope: Envelope) -> AppCommandError {
    if matches!(envelope.code, 401 | 403) {
        return AppCommandError::authentication_failed(envelope.message);
    }
    let error_code = envelope
        .data
        .get("errorCode")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    match error_code {
        Some(code) => AppCommandError::invalid_input(envelope.message).with_detail(code),
        None => AppCommandError::invalid_input(envelope.message),
    }
}

pub(super) fn retryable_agent_resolve_error(error: &AppCommandError) -> bool {
    error.code == AppErrorCode::InvalidInput
        && matches!(
            error.detail.as_deref(),
            Some(DISTRIBUTION_NOT_FOUND | ARTIFACT_NOT_READY)
        )
}
