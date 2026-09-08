use serde_json::json;

use super::capability::ResolveError;
use super::capability_schema::SchemaValidationError;

const MAX_CORRECTION_ATTEMPTS: usize = 1;
const RECOVERY_INSTRUCTION: &str = "Capability execution_status=not_started. Use the capability instructions and schema already read in this conversation, together with this error, to correct the arguments and retry the same intended operation at most once. Read through the same gateway only if you have not read that capability yet; this error does not require another read. Do not replay unchanged arguments, silently drop intended behavior, or switch capability IDs. If the corrected call fails, stop this recovery.";

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
