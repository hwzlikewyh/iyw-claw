use std::time::Duration;

use rmcp::ErrorData;
use serde_json::{json, Map, Value};

pub(super) const DEFAULT_MCP_CALL_TIMEOUT: Duration = Duration::from_secs(60);
pub(super) const MCP_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

const REMOTE_DIRECTORY_TIMEOUT: Duration = Duration::from_secs(10);
const REMOTE_INVOKE_TIMEOUT: Duration = Duration::from_secs(120);
const IMAGE_TIMEOUT: Duration = Duration::from_secs(600);
const INTERACTION_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const FETCH_MAX_TIMEOUT_SECONDS: u64 = 900;

pub(super) fn timeout_for(tool_name: &str, arguments: Option<&Map<String, Value>>) -> Duration {
    let name = tool_name.to_ascii_lowercase();
    if name.ends_with("search_iyw_capabilities") || name.ends_with("read_iyw_capability") {
        return REMOTE_DIRECTORY_TIMEOUT;
    }
    if name.ends_with("invoke_iyw_capability") || name.contains("iyw_remote_") {
        return REMOTE_INVOKE_TIMEOUT;
    }
    if name.ends_with("generate_iyw_image") {
        return image_timeout(arguments);
    }
    if name.ends_with("upload_iyw_file") {
        return UPLOAD_TIMEOUT;
    }
    if name.ends_with("fetch_iyw_url") {
        return fetch_timeout(arguments);
    }
    if name.ends_with("ask_user_question") || name.ends_with("show_interactive_html") {
        return INTERACTION_TIMEOUT;
    }
    DEFAULT_MCP_CALL_TIMEOUT
}

fn image_timeout(arguments: Option<&Map<String, Value>>) -> Duration {
    let Some(arguments) = arguments else {
        return IMAGE_TIMEOUT;
    };
    let requested = arguments
        .get("wait")
        .and_then(Value::as_object)
        .and_then(|wait| wait.get("timeoutSeconds"))
        .and_then(Value::as_u64)
        .or_else(|| {
            arguments
                .get("wait")
                .and_then(Value::as_object)
                .and_then(|wait| wait.get("timeout_seconds"))
                .and_then(Value::as_u64)
        });
    Duration::from_secs(
        requested
            .filter(|seconds| *seconds > 0)
            .unwrap_or(IMAGE_TIMEOUT.as_secs()),
    )
}

fn fetch_timeout(arguments: Option<&Map<String, Value>>) -> Duration {
    let requested = arguments
        .and_then(|args| args.get("timeout_seconds"))
        .and_then(Value::as_u64)
        .filter(|seconds| (1..=FETCH_MAX_TIMEOUT_SECONDS).contains(seconds));
    Duration::from_secs(requested.unwrap_or(DEFAULT_MCP_CALL_TIMEOUT.as_secs()))
}

pub(super) fn timeout_error(
    connection_id: &str,
    tool_name: &str,
    timeout: Duration,
    cleanup_completed: bool,
) -> ErrorData {
    tracing::warn!(
        target: "builtin_mcp",
        connection_id,
        tool = tool_name,
        timeout_ms = timeout.as_millis(),
        cleanup_completed,
        "MCP tool request exceeded its deadline"
    );
    ErrorData::internal_error(
        format!(
            "MCP tool request timed out after {} seconds",
            timeout.as_secs()
        ),
        Some(json!({
            "code": "MCP_TIMEOUT",
            "execution_status": "unknown",
            "retryable": false,
            "effectMayHaveOccurred": true,
            "cleanup": if cleanup_completed { "completed" } else { "timed_out" },
            "timeout_seconds": timeout.as_secs(),
        })),
    )
}

pub(super) fn cancelled_error(reason: &'static str) -> ErrorData {
    ErrorData::invalid_request(
        reason,
        Some(json!({
            "code": "MCP_CANCELLED",
            "execution_status": "unknown",
            "retryable": false,
            "effectMayHaveOccurred": true,
        })),
    )
}
