use rmcp::model::{CallToolResult, JsonObject, Tool};
use rmcp::ErrorData;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::acp::memory_turn::MemoryTurnTracker;
use crate::models::AgentType;
use crate::plugin_runtime::types::{PluginCallContext, PluginToolCall};

use super::capability::{CapabilityCatalog, ResolveError, ResolvedCapability};
use super::features::FeatureSnapshot;
use super::gateway_tools;
use super::plugin_catalog::PluginCapabilityRegistry;
use super::plugin_control::{self, PluginControlRequest};
use super::tool_identity::{GatewayTool, CAPABILITY_ID_MAX_CHARS};

pub(super) enum GatewayAction {
    Return(CallToolResult),
    Invoke(ResolvedCapability),
    PluginInvoke(PluginToolCall),
    PluginControl(PluginControlRequest),
}

pub(super) struct GatewaySession<'a> {
    pub connection_id: &'a str,
    pub features: &'a FeatureSnapshot,
    pub cwd: &'a Path,
    pub agent_type: AgentType,
    pub request_cancel: tokio_util::sync::CancellationToken,
    pub authority_cancel: tokio_util::sync::CancellationToken,
    pub memory_policy_loaded_nonce: Arc<AtomicU64>,
    pub memory_turn_tracker: Arc<MemoryTurnTracker>,
}

pub(super) fn tools() -> Result<Vec<Tool>, serde_json::Error> {
    gateway_tools::values()
        .into_iter()
        .map(serde_json::from_value)
        .collect()
}

pub(super) fn dispatch(
    tool: GatewayTool,
    arguments: Option<JsonObject>,
    session: GatewaySession<'_>,
) -> Result<GatewayAction, ErrorData> {
    let catalog = load_catalog()?;
    match tool {
        GatewayTool::Search => search(arguments, &catalog, &session),
        GatewayTool::Read => read(arguments, &catalog, &session),
        GatewayTool::Invoke => invoke(arguments, &catalog, session),
    }
}

fn search(
    arguments: Option<JsonObject>,
    catalog: &CapabilityCatalog,
    session: &GatewaySession<'_>,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<SearchParams>(arguments)?;
    let mut capabilities = catalog
        .search(session.features, &params.query, params.limit)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    let plugin_values = PluginCapabilityRegistry::search(
        crate::plugin_runtime::registry::global_snapshot().as_deref(),
        session.features,
        &params.query,
        session.cwd,
        session.agent_type,
        params.limit.unwrap_or(8),
    );
    let limit = params.limit.unwrap_or(8);
    let controls = plugin_control::search_summaries(&params.query)
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    let available_slots = limit.saturating_sub(controls.len());
    let plugin_slots = if plugin_values.is_empty() {
        0
    } else {
        plugin_values.len().min((available_slots + 1) / 2)
    };
    let builtin_slots = available_slots.saturating_sub(plugin_slots);
    let mut values = capabilities
        .drain(..)
        .take(builtin_slots)
        .map(|value| serde_json::to_value(value).map_err(serialize_error))
        .collect::<Result<Vec<_>, ErrorData>>()?;
    values.extend(plugin_values.into_iter().take(plugin_slots));
    values.extend(controls);
    Ok(GatewayAction::Return(CallToolResult::structured(json!({
        "capabilities": values,
        "catalog_digest": catalog_digest(catalog),
    }))))
}

fn read(
    arguments: Option<JsonObject>,
    catalog: &CapabilityCatalog,
    session: &GatewaySession<'_>,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<CapabilityParams>(arguments)?;
    let capability_id = parse_capability_id(&params.capability_id)?;
    if let Some(detail) = plugin_control::read_detail(capability_id) {
        return Ok(GatewayAction::Return(CallToolResult::structured(detail)));
    }
    let detail = catalog
        .read(session.features, capability_id)
        .map(|value| serde_json::to_value(value).map_err(serialize_error))
        .transpose()?
        .or_else(|| {
            PluginCapabilityRegistry::read(
                crate::plugin_runtime::registry::global_snapshot().as_deref(),
                capability_id,
                session.features,
                session.cwd,
                session.agent_type,
            )
        })
        .ok_or_else(unknown_capability)?;
    Ok(GatewayAction::Return(CallToolResult::structured(json!({
        "capability": detail,
        "catalog_digest": catalog_digest(catalog),
    }))))
}

fn invoke(
    arguments: Option<JsonObject>,
    catalog: &CapabilityCatalog,
    session: GatewaySession<'_>,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<InvokeParams>(arguments)?;
    let capability_id = parse_capability_id(&params.capability_id)?;
    let active_nonce = session.memory_turn_tracker.active_nonce();
    let policy_loaded_nonce = session.memory_policy_loaded_nonce.load(Ordering::Acquire);
    if memory_policy_required(capability_id, active_nonce, policy_loaded_nonce) {
        return Ok(GatewayAction::Return(CallToolResult::structured_error(
            json!({
                "code": "memory_policy_required",
                "error": "Memory policy preflight is required before this memory operation.",
                "memoryPolicyRequired": true,
                "retryable": true,
                "reason": "memory_policy_not_loaded_for_current_turn",
                "turnNonce": session.memory_turn_tracker.active_nonce(),
                "policy": {
                    "revision": crate::user_memory::MEMORY_POLICY_REVISION,
                    "digest": crate::user_memory::memory_policy_digest(),
                    "summary": crate::user_memory::MEMORY_POLICY_SUMMARY,
                    "reference": crate::user_memory::MEMORY_POLICY_REFERENCE
                }
            }),
        )));
    }
    if let Some(request) = plugin_control::parse_request(capability_id, params.arguments.clone())? {
        if params.delivery_ack.is_some() {
            return Err(ErrorData::invalid_params(
                "delivery_ack is not supported for plugin control requests",
                None,
            ));
        }
        return Ok(GatewayAction::PluginControl(request));
    }
    let arguments = Value::Object(params.arguments.clone());
    match catalog.resolve(session.features, capability_id, arguments.clone()) {
        Ok(resolved) => {
            let mut resolved = resolved;
            resolved.delivery_ack = parse_delivery_ack(params.delivery_ack)?;
            return Ok(GatewayAction::Invoke(resolved));
        }
        Err(ResolveError::Unknown) => {}
        Err(error) => return Err(resolve_error(error)),
    };
    resolve_plugin_invoke(capability_id, params.arguments, session)
}

fn memory_policy_required(
    capability_id: &str,
    active_nonce: Option<u64>,
    policy_loaded_nonce: u64,
) -> bool {
    capability_id.starts_with("iyw.memory.")
        && capability_id != "iyw.memory.policy.read.v1"
        && !active_nonce.is_some_and(|nonce| policy_loaded_nonce == nonce)
}

fn resolve_plugin_invoke(
    capability_id: &str,
    arguments: Map<String, Value>,
    session: GatewaySession<'_>,
) -> Result<GatewayAction, ErrorData> {
    let plugin = PluginCapabilityRegistry::read(
        crate::plugin_runtime::registry::global_snapshot().as_deref(),
        capability_id,
        session.features,
        session.cwd,
        session.agent_type,
    )
    .ok_or_else(unknown_capability)?;
    if plugin.get("status").and_then(Value::as_str) != Some("available") {
        let reason = plugin
            .get("unavailable_reason")
            .and_then(Value::as_str)
            .unwrap_or("plugin_unavailable");
        return Err(ErrorData::invalid_params(
            "plugin capability is unavailable for this session",
            Some(json!({"code": reason})),
        ));
    }
    let plugin_slug = plugin
        .get("plugin_slug")
        .and_then(Value::as_str)
        .ok_or_else(unknown_capability)?
        .to_string();
    let context = plugin_call_context(&plugin_slug, capability_id, session);
    Ok(GatewayAction::PluginInvoke(PluginToolCall {
        context,
        arguments,
    }))
}

fn plugin_call_context(
    plugin_slug: &str,
    capability_id: &str,
    session: GatewaySession<'_>,
) -> PluginCallContext {
    let context = PluginCallContext {
        connection_id: session.connection_id.to_string(),
        plugin_slug: plugin_slug.to_string(),
        capability_id: capability_id.to_string(),
        workspace_key: crate::commands::skill_inventory::workspace_key(Some(
            session.cwd.to_string_lossy().as_ref(),
        )),
        workspace_dir: session.cwd.to_path_buf(),
        agent_type: session.agent_type,
        host_gateway_supported: crate::acp::connection::agent_supports_builtin_mcp(
            session.agent_type,
        ),
        cancellation: session.request_cancel,
        authority_cancellation: session.authority_cancel,
        permission_revision: crate::plugin_runtime::registry::global_snapshot()
            .and_then(|snapshot| {
                snapshot
                    .plugins
                    .get(plugin_slug)
                    .map(|plugin| plugin.permissions_digest.clone())
            })
            .unwrap_or_default(),
    };
    context
}

fn serialize_error(error: serde_json::Error) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

fn catalog_digest(catalog: &CapabilityCatalog) -> String {
    let plugin_digest = crate::plugin_runtime::registry::global_snapshot()
        .map(|snapshot| snapshot.digest.clone())
        .unwrap_or_default();
    format!("{}:{}", catalog.digest(), plugin_digest)
}

fn parse_delivery_ack(value: Option<String>) -> Result<Option<String>, ErrorData> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.chars().count() > CAPABILITY_ID_MAX_CHARS {
        return Err(ErrorData::invalid_params(
            "delivery_ack must be a non-empty receipt",
            None,
        ));
    }
    Ok(Some(value.to_string()))
}

fn parse_capability_id(value: &str) -> Result<&str, ErrorData> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > CAPABILITY_ID_MAX_CHARS {
        return Err(ErrorData::invalid_params(
            "capability_id must contain between 1 and 128 characters",
            None,
        ));
    }
    Ok(value)
}

fn load_catalog() -> Result<CapabilityCatalog, ErrorData> {
    CapabilityCatalog::load().map_err(|error| {
        tracing::error!(
            target: "builtin_mcp",
            error = %error,
            "failed to load capability catalog"
        );
        ErrorData::internal_error("failed to load capability catalog", None)
    })
}

fn parse<T: DeserializeOwned>(arguments: Option<JsonObject>) -> Result<T, ErrorData> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

fn unknown_capability() -> ErrorData {
    ErrorData::invalid_params("unknown capability id", None)
}

fn resolve_error(error: ResolveError) -> ErrorData {
    ErrorData::invalid_params(error.to_string(), None)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    query: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityParams {
    capability_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvokeParams {
    capability_id: String,
    arguments: Map<String, Value>,
    delivery_ack: Option<String>,
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
