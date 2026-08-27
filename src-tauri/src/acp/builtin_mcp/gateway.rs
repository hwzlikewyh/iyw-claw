use rmcp::model::{CallToolResult, JsonObject, Tool};
use rmcp::ErrorData;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::Path;

use crate::models::AgentType;
use crate::plugin_runtime::types::{PluginCallContext, PluginToolCall};

use super::capability::{CapabilityCatalog, ResolveError, ResolvedCapability};
use super::features::FeatureSnapshot;
use super::plugin_catalog::PluginCapabilityRegistry;
use super::tool_identity::{
    GatewayTool, CAPABILITY_ID_MAX_CHARS, INVOKE_TOOL, READ_TOOL, SEARCH_TOOL,
};

pub(super) const PLUGIN_INSTALL_CAPABILITY: &str = "iyw.plugins.install.request.v1";

pub(super) enum GatewayAction {
    Return(CallToolResult),
    Invoke(ResolvedCapability),
    PluginInvoke(PluginToolCall),
    PluginInstallRequest(PluginInstallRequest),
}

pub(super) struct PluginInstallRequest {
    pub skill_id: String,
    pub version: String,
    pub plugin_name: String,
}

pub(super) fn tools() -> Result<Vec<Tool>, serde_json::Error> {
    [search_tool(), read_tool(), invoke_tool()]
        .into_iter()
        .map(serde_json::from_value)
        .collect()
}

pub(super) fn dispatch(
    tool: GatewayTool,
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    cwd: &Path,
    agent_type: AgentType,
    request_cancel: tokio_util::sync::CancellationToken,
    authority_cancel: tokio_util::sync::CancellationToken,
) -> Result<GatewayAction, ErrorData> {
    let catalog = load_catalog()?;
    match tool {
        GatewayTool::Search => search(arguments, features, &catalog, cwd, agent_type),
        GatewayTool::Read => read(arguments, features, &catalog, cwd, agent_type),
        GatewayTool::Invoke => invoke(
            arguments,
            features,
            &catalog,
            cwd,
            agent_type,
            request_cancel,
            authority_cancel,
        ),
    }
}

fn search(
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    catalog: &CapabilityCatalog,
    cwd: &Path,
    agent_type: AgentType,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<SearchParams>(arguments)?;
    let mut capabilities = catalog
        .search(features, &params.query, params.limit)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    let plugin_values = PluginCapabilityRegistry::search(
        crate::plugin_runtime::registry::global_snapshot().as_deref(),
        features,
        &params.query,
        cwd,
        agent_type,
        params.limit.unwrap_or(8),
    );
    let limit = params.limit.unwrap_or(8);
    let install_required = is_plugin_install_query(&params.query);
    let install_slots = usize::from(install_required);
    let available_slots = limit.saturating_sub(install_slots);
    let plugin_slots = if plugin_values.is_empty() {
        0
    } else {
        plugin_values.len().min((available_slots + 1) / 2)
    };
    let builtin_slots = limit.saturating_sub(install_slots + plugin_slots);
    let mut values = capabilities
        .drain(..)
        .take(builtin_slots)
        .map(|value| serde_json::to_value(value).map_err(serialize_error))
        .collect::<Result<Vec<_>, ErrorData>>()?;
    values.extend(plugin_values.into_iter().take(plugin_slots));
    if install_required {
        values.push(plugin_install_capability_summary());
    }
    Ok(GatewayAction::Return(CallToolResult::structured(json!({
        "capabilities": values,
        "catalog_digest": catalog_digest(catalog),
    }))))
}

fn read(
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    catalog: &CapabilityCatalog,
    cwd: &Path,
    agent_type: AgentType,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<CapabilityParams>(arguments)?;
    let capability_id = parse_capability_id(&params.capability_id)?;
    if capability_id == PLUGIN_INSTALL_CAPABILITY {
        return Ok(GatewayAction::Return(CallToolResult::structured(
            plugin_install_capability_detail(),
        )));
    }
    let detail = catalog
        .read(features, capability_id)
        .map(|value| serde_json::to_value(value).map_err(serialize_error))
        .transpose()?
        .or_else(|| {
            PluginCapabilityRegistry::read(
                crate::plugin_runtime::registry::global_snapshot().as_deref(),
                capability_id,
                features,
                cwd,
                agent_type,
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
    features: &FeatureSnapshot,
    catalog: &CapabilityCatalog,
    cwd: &Path,
    agent_type: AgentType,
    request_cancel: tokio_util::sync::CancellationToken,
    authority_cancel: tokio_util::sync::CancellationToken,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<InvokeParams>(arguments)?;
    let capability_id = parse_capability_id(&params.capability_id)?;
    if capability_id == PLUGIN_INSTALL_CAPABILITY {
        if params.delivery_ack.is_some() {
            return Err(ErrorData::invalid_params(
                "delivery_ack is not supported for plugin installation requests",
                None,
            ));
        }
        return plugin_install_request(params.arguments);
    }
    let arguments = Value::Object(params.arguments);
    match catalog.resolve(features, capability_id, arguments.clone()) {
        Ok(resolved) => {
            let mut resolved = resolved;
            resolved.delivery_ack = parse_delivery_ack(params.delivery_ack)?;
            return Ok(GatewayAction::Invoke(resolved));
        }
        Err(ResolveError::Unknown) => {}
        Err(error) => return Err(resolve_error(error)),
    };
    let plugin = PluginCapabilityRegistry::read(
        crate::plugin_runtime::registry::global_snapshot().as_deref(),
        capability_id,
        features,
        cwd,
        agent_type,
    )
    .ok_or_else(unknown_capability)?;
    if plugin.get("status").and_then(Value::as_str) != Some("available") {
        return Err(ErrorData::invalid_params(
            "plugin capability is unavailable for this session",
            None,
        ));
    }
    let plugin_slug = plugin
        .get("plugin_slug")
        .and_then(Value::as_str)
        .ok_or_else(unknown_capability)?
        .to_string();
    let context = PluginCallContext {
        plugin_slug,
        capability_id: capability_id.to_string(),
        workspace_key: cwd.to_string_lossy().to_string(),
        workspace_dir: cwd.to_path_buf(),
        agent_type,
        host_gateway_supported: crate::acp::connection::agent_supports_builtin_mcp(agent_type),
        cancellation: request_cancel,
        authority_cancellation: authority_cancel,
        permission_revision: crate::plugin_runtime::registry::global_snapshot()
            .and_then(|snapshot| snapshot.plugins.get(plugin.get("plugin_slug")?.as_str()?))
            .map(|plugin| plugin.permissions_digest.clone())
            .unwrap_or_default(),
    };
    return Ok(GatewayAction::PluginInvoke(PluginToolCall {
        context,
        arguments: params.arguments,
    }));
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

fn is_plugin_install_query(query: &str) -> bool {
    let lowered = query.to_ascii_lowercase();
    lowered.contains("plugin") && (lowered.contains("install") || query.contains("安装"))
}

fn plugin_install_capability_summary() -> Value {
    json!({
        "capability_id": PLUGIN_INSTALL_CAPABILITY,
        "summary": "Request explicit user approval before downloading a verified plugin.",
        "category": "plugin",
        "aliases": ["install plugin", "安装插件"],
        "intent_terms": ["plugin", "install", "安装", "插件"],
        "when_to_use": "Use only when a concrete plugin and version are known and it is not installed.",
        "required_inputs": ["skill_id", "version", "plugin_name"],
        "schema_digest": "builtin:plugin-install-request:v1",
        "status": "install_required"
    })
}

fn plugin_install_capability_detail() -> Value {
    json!({
        "capability": {
            "capability_id": PLUGIN_INSTALL_CAPABILITY,
            "description": "Request explicit user approval before downloading a verified plugin. Refuses implicit downloads.",
            "input_schema": {
                "type": "object",
                "required": ["skill_id", "version", "plugin_name"],
                "properties": {
                    "skill_id": {"type": "string"},
                    "version": {"type": "string"},
                    "plugin_name": {"type": "string"},
                },
                "additionalProperties": false
            },
            "category": "plugin",
            "aliases": ["install plugin", "安装插件"],
            "intent_terms": ["plugin", "install", "安装", "插件"],
            "when_to_use": "Use only when a concrete plugin and version are known and it is not installed.",
            "required_inputs": ["skill_id", "version", "plugin_name"],
            "schema_digest": "builtin:plugin-install-request:v1",
            "status": "available"
        },
        "catalog_digest": "builtin:plugin-install-request:v1"
    })
}

fn plugin_install_request(arguments: Map<String, Value>) -> Result<GatewayAction, ErrorData> {
    let skill_id = bounded_string(&arguments, "skill_id", 64)?;
    let version = bounded_string(&arguments, "version", 64)?;
    let plugin_name = bounded_string(&arguments, "plugin_name", 128)?;
    Ok(GatewayAction::PluginInstallRequest(PluginInstallRequest {
        skill_id,
        version,
        plugin_name,
    }))
}

fn bounded_string(
    arguments: &Map<String, Value>,
    name: &str,
    max_chars: usize,
) -> Result<String, ErrorData> {
    let value = arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= max_chars)
        .ok_or_else(|| ErrorData::invalid_params(format!("{name} is required"), None))?;
    Ok(value.to_string())
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

fn search_tool() -> Value {
    json!({
        "name": SEARCH_TOOL,
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Proactively search the current session's IYW capability catalog when a concrete goal needs host-side state or action, especially delegation, submitting feedback or user questions, session state, image or media work, task artifacts, persistent memory, current user profile, channels, or automation. Prior decisions, preferences, repeated workflows, or earlier context make task-scoped memory recall a concrete subgoal. A final user-facing file, directory, or public URL makes Artifact registration a required subgoal before completion. Search once before claiming such a step is unavailable or asking the user to do it manually when no direct tool fits. A user-requested exact visible direct tool takes precedence only for the subgoal it fully satisfies; apply discovery independently to remaining host-side subgoals. Ask for a missing primary object before search. Use two to five discriminating action/object keywords; normalized Chinese and English intent terms are accepted. Do not search greetings, ordinary questions, self-contained trivial tasks, current-turn-only context, every turn, or merely to enumerate capabilities. Read at most two plausible candidates per result set. An empty result, no plausible candidate, or two non-matches permits the single search retry.",
        "inputSchema": {
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string", "minLength": 1, "maxLength": 256 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20, "default": 8 }
            },
            "additionalProperties": false
        }
    })
}

fn read_tool() -> Value {
    json!({
        "name": READ_TOOL,
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Read the full description and current input schema for one exact stable capability id returned by this session's search. Read before invoking and obey the returned schema. Ask for missing referenced objects or required inputs; never guess ids, paths, URLs, field names, or arguments.",
        "inputSchema": {
            "type": "object",
            "required": ["capability_id"],
            "properties": {
                "capability_id": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": CAPABILITY_ID_MAX_CHARS
                }
            },
            "additionalProperties": false
        }
    })
}

fn invoke_tool() -> Value {
    json!({
        "name": INVOKE_TOOL,
        "description": "Call this gateway role only through the exact current callable identity and surface that advertised it. On an unknown, unsupported, or not-found routing error, stop this gateway for the turn and never retry through another name or surface. Invoke an available IYW capability using an exact stable id returned by this session's search. Supply arguments exactly as described by read_iyw_capability. If the id becomes unavailable or routing fails, do not retry under a guessed id or namespace. If a prior response returned iyw_delivery_receipt and a later real invocation is needed, echo it only as top-level delivery_ack; never put it in arguments or fabricate an invocation just to acknowledge it.",
        "inputSchema": {
            "type": "object",
            "required": ["capability_id", "arguments"],
            "properties": {
                "capability_id": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": CAPABILITY_ID_MAX_CHARS
                },
                "arguments": { "type": "object" },
                "delivery_ack": { "type": "string", "minLength": 1, "maxLength": 128 }
            },
            "additionalProperties": false
        }
    })
}
