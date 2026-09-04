use std::time::Duration;

use serde_json::Value;

use super::agent_browser_request::OpencliRequest;
use super::agent_tool_support::invalid_argument;
use super::error::BrowserError;

const MAX_ARGUMENTS: usize = 48;
const MAX_ARGUMENT_CHARS: usize = 8_192;
const MAX_TOTAL_ARGUMENT_CHARS: usize = 64 * 1_024;
const RESERVED_OPENCLI_ARGUMENTS: &[&str] = &["--profile", "--session", "--tab", "--window"];

pub(super) fn requested_target(input: &Value) -> Option<String> {
    input
        .get("tab_id")
        .or_else(|| input.get("tabId"))
        .and_then(Value::as_str)
        .and_then(opencli_target)
}

pub(super) fn requested_timeout(input: &Value) -> Duration {
    Duration::from_millis(
        input
            .get("timeout_ms")
            .or_else(|| input.get("timeoutMs"))
            .and_then(Value::as_u64)
            .unwrap_or(60_000)
            .clamp(1_000, 300_000),
    )
}

pub(super) fn request(
    command: &str,
    args: Vec<String>,
    target: Option<String>,
    timeout: Duration,
) -> Result<OpencliRequest, BrowserError> {
    Ok(OpencliRequest {
        command: command.to_string(),
        args,
        target,
        timeout,
    })
}

pub(super) fn advanced_arguments(input: &Value) -> Result<Vec<String>, BrowserError> {
    let Some(values) = input.get("arguments") else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .filter(|values| values.len() <= MAX_ARGUMENTS)
        .ok_or_else(|| invalid_argument("Invalid browser command arguments"))?;
    let mut total = 0_usize;
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .filter(|value| !value.contains('\0'))
                .ok_or_else(|| invalid_argument("Invalid browser command argument"))?;
            total = total.saturating_add(value.chars().count());
            if value.chars().count() > MAX_ARGUMENT_CHARS
                || total > MAX_TOTAL_ARGUMENT_CHARS
                || reserved_opencli_argument(value)
            {
                return Err(invalid_argument(
                    "Unsafe or oversized browser command argument",
                ));
            }
            Ok(value.to_string())
        })
        .collect()
}

pub(super) fn validate_target(value: &str, max_chars: usize) -> Result<(), BrowserError> {
    if value.is_empty() || value.contains('\0') || value.chars().count() > max_chars {
        return Err(invalid_argument("Invalid browser target"));
    }
    Ok(())
}

fn opencli_target(value: &str) -> Option<String> {
    let Some(value) = value.strip_prefix("opencli:") else {
        return Some(value.to_string());
    };
    value
        .split_once(':')
        .map(|(_, target)| target.to_string())
        .filter(|target| !target.is_empty())
}

fn reserved_opencli_argument(value: &str) -> bool {
    RESERVED_OPENCLI_ARGUMENTS.iter().any(|reserved| {
        value.eq_ignore_ascii_case(reserved)
            || value
                .get(..reserved.len().saturating_add(1))
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&format!("{reserved}=")))
    })
}
