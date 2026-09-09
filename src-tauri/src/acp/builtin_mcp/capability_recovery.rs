use rmcp::model::CallToolRequestParams;
use rmcp::ErrorData;
use serde_json::json;

use super::capability::ResolveError;
use super::capability_schema::SchemaValidationError;
use super::tool_identity::{resolve_gateway_route, GatewayRoute, GatewayTool, IMAGE_TOOL};

const MAX_CORRECTION_ATTEMPTS: usize = 1;
const RECOVERY_INSTRUCTION: &str = "Capability execution_status=not_started. Use the capability instructions and schema already read in this conversation, together with this error, to correct the arguments and retry the same intended operation at most once. Read through the same gateway only if you have not read that capability yet; this error does not require another read. Do not replay unchanged arguments, silently drop intended behavior, or switch capability IDs. If the corrected call fails, stop this recovery.";

pub(super) fn direct_tool_hint(
    request: &CallToolRequestParams,
    server_name: &str,
) -> Option<&'static str> {
    let id = request
        .arguments
        .as_ref()?
        .get("capability_id")?
        .as_str()?
        .trim();
    // 仅识别已观察到的误用和真实工具名，不按相似名称推断能力。
    if id == "iyw.image.generate.v1" {
        return Some(IMAGE_TOOL);
    }
    resolve_gateway_route(id, server_name)
        .map(GatewayRoute::tool)
        .map(GatewayTool::name)
}

pub(super) fn annotate_lookup_error(
    mut error: ErrorData,
    direct_tool: Option<&'static str>,
) -> ErrorData {
    if error.message != "unknown capability id" {
        return error;
    }
    let guidance = match direct_tool {
        Some(tool) => format!(
            "This is a direct-tool task. Read the advertised {tool} definition and use its exact callable identity; do not construct a capability_id for it."
        ),
        None => "A capability_id must be copied exactly from this session's search result or the advertised memory operation mapping. Stop guessing IDs, adding version suffixes, or trying other namespaces.".to_string(),
    };
    error.message = format!(
        "unknown capability id. {guidance} This lookup did not execute a business operation. That does not establish the outcome of earlier calls: do not replay an earlier rejected, timed-out, or effect-unknown operation through a different name or tool."
    ).into();
    error.data = Some(json!({
        "code": "capability_not_found",
        "execution_status": "not_started",
        "retryable": false,
        "recovery": {
            "action": if direct_tool.is_some() { "use_advertised_direct_tool" } else { "use_catalog_evidence" },
            "tool_name": direct_tool,
            "hint": guidance,
            "retry_unchanged_arguments": false,
            "replay_previous_operation": false
        }
    }));
    error
}

pub(super) fn invalid_arguments(
    capability_id: &str,
    schema_digest: &str,
    validation: SchemaValidationError,
) -> ResolveError {
    let mut message = format!("{capability_id}: {validation}.");
    if let Some(properties) = &validation.allowed_properties {
        message.push_str(&format!(" Allowed properties: {}.", json!(properties)));
    }
    if let Some(hint) = &validation.hint {
        message.push_str(&format!(" {hint}"));
    }
    message.push_str(&format!(" {RECOVERY_INSTRUCTION}"));
    // 仅记录目录元数据和校验原因，不记录参数值或用户提供的字段名。
    tracing::warn!(
        target: "builtin_mcp",
        capability_id,
        schema_digest,
        reason = %validation.message,
        execution_status = "not_started",
        "[MCP][gateway] capability arguments rejected before execution"
    );
    ResolveError::InvalidArguments {
        message,
        data: json!({
            "code": "capability_schema_mismatch",
            "capability_id": capability_id,
            "schema_digest": schema_digest,
            "execution_status": "not_started",
            "validation": validation,
            "recovery": {
                "action": "use_schema_and_correct_arguments",
                "max_attempts": MAX_CORRECTION_ATTEMPTS,
                "retry_unchanged_arguments": false
            }
        }),
    }
}
