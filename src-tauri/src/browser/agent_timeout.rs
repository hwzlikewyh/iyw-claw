use std::time::Duration;

use serde_json::Value;

use super::agent_tool_support::invalid_argument;
use super::BrowserError;

const MAX_COMMAND_TIMEOUT_MS: u64 = 300_000;

pub(super) fn requested_timeout(
    input: &Value,
    default: Duration,
) -> Result<Duration, BrowserError> {
    let Some(value) = input.get("timeout_ms").or_else(|| input.get("timeoutMs")) else {
        return Ok(default);
    };
    value
        .as_u64()
        .filter(|value| (1..=MAX_COMMAND_TIMEOUT_MS).contains(value))
        .map(Duration::from_millis)
        .ok_or_else(|| invalid_argument("Invalid browser command timeout"))
}
