use super::Envelope;
use crate::app_error::AppCommandError;

pub(super) fn envelope_error(envelope: Envelope) -> AppCommandError {
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
