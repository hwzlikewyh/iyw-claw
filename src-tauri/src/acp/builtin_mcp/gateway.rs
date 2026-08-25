use rmcp::model::{CallToolResult, JsonObject, Tool};
use rmcp::ErrorData;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::capability::{CapabilityCatalog, ResolveError, ResolvedCapability};
use super::capability_registry::tool_name_for_capability_id;
use super::features::FeatureSnapshot;

pub(super) const SEARCH_TOOL: &str = "search_iyw_capabilities";
pub(super) const READ_TOOL: &str = "read_iyw_capability";
pub(super) const INVOKE_TOOL: &str = "invoke_iyw_capability";

const MAX_GATEWAY_WRAPPER_DEPTH: u8 = 4;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GatewayToolIdentity {
    NotGateway,
    UnresolvedGateway,
    Resolved(String),
}

pub(super) enum GatewayAction {
    Return(CallToolResult),
    Invoke(ResolvedCapability),
}

pub(crate) fn invoked_tool_name(raw_input: &Option<String>) -> GatewayToolIdentity {
    let Some(raw_input) = raw_input.as_deref() else {
        return GatewayToolIdentity::NotGateway;
    };
    if !raw_input.contains(INVOKE_TOOL)
        && (!raw_input.contains("capability_id") || !raw_input.contains("arguments"))
    {
        return GatewayToolIdentity::NotGateway;
    }
    let Ok(value) = serde_json::from_str::<Value>(raw_input) else {
        return GatewayToolIdentity::NotGateway;
    };
    gateway_tool_identity(&value, 0)
}

fn gateway_tool_identity(value: &Value, depth: u8) -> GatewayToolIdentity {
    if depth > MAX_GATEWAY_WRAPPER_DEPTH {
        return GatewayToolIdentity::NotGateway;
    }
    if let Some(encoded) = value.as_str() {
        return serde_json::from_str::<Value>(encoded)
            .map_or(GatewayToolIdentity::NotGateway, |decoded| {
                gateway_tool_identity(&decoded, depth + 1)
            });
    }
    let Some(object) = value.as_object() else {
        return GatewayToolIdentity::NotGateway;
    };
    if let Some(tool_name) = object.get("toolName") {
        return codebuddy_gateway_identity(tool_name, object.get("params"), depth);
    }
    direct_gateway_identity(value)
}

fn codebuddy_gateway_identity(
    tool_name: &Value,
    params: Option<&Value>,
    depth: u8,
) -> GatewayToolIdentity {
    let Some(tool_name) = tool_name.as_str() else {
        return GatewayToolIdentity::NotGateway;
    };
    if canonical_name(tool_name.trim()) != Some(INVOKE_TOOL) {
        return GatewayToolIdentity::NotGateway;
    }
    let Some(params) = params else {
        return GatewayToolIdentity::UnresolvedGateway;
    };
    match gateway_tool_identity(params, depth + 1) {
        GatewayToolIdentity::Resolved(tool_name) => GatewayToolIdentity::Resolved(tool_name),
        _ => GatewayToolIdentity::UnresolvedGateway,
    }
}

fn direct_gateway_identity(value: &Value) -> GatewayToolIdentity {
    let Ok(params) = serde_json::from_value::<InvokeParams>(value.clone()) else {
        return GatewayToolIdentity::NotGateway;
    };
    let Ok(capability_id) = parse_capability_id(&params.capability_id) else {
        return GatewayToolIdentity::NotGateway;
    };
    tool_name_for_capability_id(capability_id)
        .map_or(GatewayToolIdentity::UnresolvedGateway, |tool_name| {
            GatewayToolIdentity::Resolved(tool_name.to_string())
        })
}

pub(super) fn tools() -> Result<Vec<Tool>, serde_json::Error> {
    [search_tool(), read_tool(), invoke_tool()]
        .into_iter()
        .map(serde_json::from_value)
        .collect()
}

pub(super) fn dispatch(
    raw_name: &str,
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    server_name: &str,
) -> Result<GatewayAction, ErrorData> {
    let catalog = load_catalog()?;
    match authorized_name(raw_name, server_name) {
        Some(SEARCH_TOOL) => search(arguments, features, &catalog),
        Some(READ_TOOL) => read(arguments, features, &catalog),
        Some(INVOKE_TOOL) => invoke(arguments, features, &catalog),
        _ => Err(ErrorData::invalid_params("unknown MCP gateway tool", None)),
    }
}

fn search(
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    catalog: &CapabilityCatalog,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<SearchParams>(arguments)?;
    let capabilities = catalog
        .search(features, &params.query, params.limit)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    Ok(GatewayAction::Return(CallToolResult::structured(json!({
        "capabilities": capabilities,
        "catalog_digest": catalog.digest(),
    }))))
}

fn read(
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    catalog: &CapabilityCatalog,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<CapabilityParams>(arguments)?;
    let capability_id = parse_capability_id(&params.capability_id)?;
    let detail = catalog
        .read(features, capability_id)
        .ok_or_else(unknown_capability)?;
    Ok(GatewayAction::Return(CallToolResult::structured(json!({
        "capability": detail,
        "catalog_digest": catalog.digest(),
    }))))
}

fn invoke(
    arguments: Option<JsonObject>,
    features: &FeatureSnapshot,
    catalog: &CapabilityCatalog,
) -> Result<GatewayAction, ErrorData> {
    let params = parse::<InvokeParams>(arguments)?;
    let capability_id = parse_capability_id(&params.capability_id)?;
    let arguments = Value::Object(params.arguments);
    let mut resolved = catalog
        .resolve(features, capability_id, arguments)
        .map_err(resolve_error)?;
    resolved.delivery_ack = parse_delivery_ack(params.delivery_ack)?;
    Ok(GatewayAction::Invoke(resolved))
}

fn parse_delivery_ack(value: Option<String>) -> Result<Option<String>, ErrorData> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 128 {
        return Err(ErrorData::invalid_params(
            "delivery_ack must be a non-empty receipt",
            None,
        ));
    }
    Ok(Some(value.to_string()))
}

fn parse_capability_id(value: &str) -> Result<&str, ErrorData> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 128 {
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

fn canonical_name(raw: &str) -> Option<&'static str> {
    let name = raw.rsplit("__").next().unwrap_or(raw);
    bare_gateway_name(name)
}

fn authorized_name(raw: &str, server_name: &str) -> Option<&'static str> {
    if let Some(name) = bare_gateway_name(raw) {
        return Some(name);
    }
    let mcp_prefix = format!("mcp__{server_name}__");
    let host_prefix = format!("{server_name}__");
    let name = raw
        .strip_prefix(&mcp_prefix)
        .or_else(|| raw.strip_prefix(&host_prefix))?;
    bare_gateway_name(name)
}

fn bare_gateway_name(name: &str) -> Option<&'static str> {
    match name {
        SEARCH_TOOL => Some(SEARCH_TOOL),
        READ_TOOL => Some(READ_TOOL),
        INVOKE_TOOL => Some(INVOKE_TOOL),
        _ => None,
    }
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
                "capability_id": { "type": "string", "minLength": 1, "maxLength": 128 }
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
                "capability_id": { "type": "string", "minLength": 1, "maxLength": 128 },
                "arguments": { "type": "object" },
                "delivery_ack": { "type": "string", "minLength": 1, "maxLength": 128 }
            },
            "additionalProperties": false
        }
    })
}
