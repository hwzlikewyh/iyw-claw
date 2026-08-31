use std::time::Instant;

use rmcp::model::{CallToolRequestParams, CallToolResult, JsonObject};
use rmcp::ErrorData;
use sha2::{Digest, Sha256};

use super::authority::SessionContext;
use super::tool_identity::{GatewayRoute, GatewayTool, CAPABILITY_ID_MAX_CHARS};

const FINGERPRINT_HEX_CHARS: usize = 12;

pub(super) struct GatewayCallTrace {
    started: Instant,
    connection_id: String,
    agent: String,
    server_fingerprint: String,
    name_fingerprint: String,
    name_chars: usize,
    role: Option<GatewayTool>,
    identity_form: Option<&'static str>,
    capability_ref: String,
}

struct CallOutcome {
    stage: &'static str,
    outcome: &'static str,
    error_code: String,
}

impl GatewayCallTrace {
    pub(super) fn new(
        authority: &SessionContext,
        request: &CallToolRequestParams,
        route: Option<GatewayRoute>,
    ) -> Self {
        let raw_name = request.name.as_ref();
        Self {
            started: Instant::now(),
            connection_id: authority.connection_id().to_string(),
            agent: authority.agent_type().to_string(),
            server_fingerprint: fingerprint(authority.gateway_server_name()),
            name_fingerprint: fingerprint(raw_name),
            name_chars: raw_name.chars().count(),
            role: route.map(GatewayRoute::tool),
            identity_form: route.map(|item| item.form().label()),
            capability_ref: safe_capability_ref(&request.arguments),
        }
    }

    pub(super) fn log_received(&self) {
        let role = self.role.map(GatewayTool::name).unwrap_or("unknown");
        let identity_form = self.identity_form.unwrap_or("unknown");
        tracing::info!(
            target: "builtin_mcp",
            connection_id = %self.connection_id,
            agent = %self.agent,
            role,
            identity_form,
            server_fingerprint = %self.server_fingerprint,
            name_fingerprint = %self.name_fingerprint,
            name_chars = self.name_chars,
            capability_ref = %self.capability_ref,
            "[MCP][gateway] call received"
        );
    }

    pub(super) fn log_result(&self, result: &CallToolResult) {
        let outcome = if result.is_error == Some(true) {
            "tool_error"
        } else {
            "success"
        };
        let stage = match self.role {
            Some(GatewayTool::Search) => "capability_search",
            Some(GatewayTool::Read) => "schema_read",
            Some(GatewayTool::Invoke) => "capability_invoke",
            _ => "completed",
        };
        let error_code = result
            .structured_content
            .as_ref()
            .and_then(|value| value.get("code"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.log_outcome(CallOutcome {
            stage,
            outcome,
            error_code,
        });
    }

    pub(super) fn log_error(&self, stage: &'static str, error: &ErrorData) {
        self.log_outcome(CallOutcome {
            stage,
            outcome: "error",
            error_code: format!("{:?}", error.code),
        });
    }

    fn log_outcome(&self, result: CallOutcome) {
        tracing::info!(
            target: "builtin_mcp",
            connection_id = %self.connection_id,
            role = self.role.map(GatewayTool::name).unwrap_or("unknown"),
            capability_ref = %self.capability_ref,
            stage = result.stage,
            outcome = result.outcome,
            error_code = %result.error_code,
            elapsed_ms = self.started.elapsed().as_millis(),
            "[MCP][gateway] call finished"
        );
    }
}

pub(super) fn gateway_error_stage(error: &ErrorData) -> &'static str {
    let message: &str = error.message.as_ref();
    if message == "unknown capability id" {
        return "capability_unknown";
    }
    if message.contains("capability is unavailable") || message.contains("not enabled") {
        return "capability_unavailable";
    }
    if message.contains("arguments do not match") {
        return "schema_validation";
    }
    "gateway_arguments"
}

pub(super) fn invocation_error_stage(error: &ErrorData) -> &'static str {
    let message = error.message.to_ascii_lowercase();
    if message.contains("cancel") {
        return "cancelled";
    }
    if message.contains("authority revoked") {
        return "authority_revoked";
    }
    "backend"
}

pub(super) fn log_tools_list(authority: &SessionContext, tool_count: usize) {
    tracing::info!(
        target: "builtin_mcp",
        connection_id = authority.connection_id(),
        agent = %authority.agent_type(),
        server_fingerprint = fingerprint(authority.gateway_server_name()),
        tool_count,
        "[MCP][gateway] tools listed"
    );
}

pub(super) fn log_http_rejection(
    stage: &'static str,
    status: axum::http::StatusCode,
    connection_id: Option<&str>,
) {
    tracing::warn!(
        target: "builtin_mcp",
        stage,
        status = status.as_u16(),
        connection_id = connection_id.unwrap_or("unavailable"),
        "[MCP][http] request rejected"
    );
}

fn safe_capability_ref(arguments: &Option<JsonObject>) -> String {
    let Some(value) = arguments
        .as_ref()
        .and_then(|items| items.get("capability_id"))
        .and_then(serde_json::Value::as_str)
    else {
        return "none".to_string();
    };
    if value.starts_with("iyw.")
        && value.chars().count() <= CAPABILITY_ID_MAX_CHARS
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return value.to_string();
    }
    format!("hash:{}", fingerprint(value))
}

fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{:x}", digest)[..FINGERPRINT_HEX_CHARS].to_string()
}
