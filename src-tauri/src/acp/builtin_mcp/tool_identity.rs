use serde::Deserialize;
use serde_json::{Map, Value};

use super::capability_registry::tool_name_for_capability_id;

pub(super) const SEARCH_TOOL: &str = "search_iyw_capabilities";
pub(super) const READ_TOOL: &str = "read_iyw_capability";
pub(super) const INVOKE_TOOL: &str = "invoke_iyw_capability";
pub(super) const IMAGE_TOOL: &str = "generate_iyw_image";
pub(super) const KNOWLEDGE_TOOL: &str = "search_iyw_knowledge";
pub(super) const MEMORY_TOOL: &str = "manage_iyw_memory";
pub(super) const CAPABILITY_ID_MAX_CHARS: usize = 128;

const MAX_GATEWAY_WRAPPER_DEPTH: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GatewayTool {
    Search,
    Read,
    Invoke,
    Image,
    Knowledge,
    Memory,
}

impl GatewayTool {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Search => SEARCH_TOOL,
            Self::Read => READ_TOOL,
            Self::Invoke => INVOKE_TOOL,
            Self::Image => IMAGE_TOOL,
            Self::Knowledge => KNOWLEDGE_TOOL,
            Self::Memory => MEMORY_TOOL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GatewayNameForm {
    Bare,
    McpQualified,
    HostQualified,
    McpNormalized,
    HostNormalized,
}

impl GatewayNameForm {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Bare => "bare",
            Self::McpQualified => "mcp_qualified",
            Self::HostQualified => "host_qualified",
            Self::McpNormalized => "mcp_normalized",
            Self::HostNormalized => "host_normalized",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GatewayRoute {
    tool: GatewayTool,
    form: GatewayNameForm,
}

impl GatewayRoute {
    pub(super) fn tool(self) -> GatewayTool {
        self.tool
    }

    pub(super) fn form(self) -> GatewayNameForm {
        self.form
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GatewayToolIdentity {
    NotGateway,
    UnresolvedGateway,
    Resolved(String),
}

/// 仅解析当前 authority 绑定的 server identity。
/// 部分 Agent 会把 MCP server 名中的连字符规范化为下划线后再传入 tools/call，
/// 因此同时接受该确定性别名，但不接受外来或历史 namespace。
pub(super) fn resolve_gateway_route(raw: &str, server_name: &str) -> Option<GatewayRoute> {
    if let Some(tool) = bare_gateway_tool(raw) {
        return Some(GatewayRoute {
            tool,
            form: GatewayNameForm::Bare,
        });
    }
    if let Some(tool) = qualified_tool(raw, server_name, false) {
        return Some(tool);
    }
    let normalized = normalize_server_name(server_name);
    (normalized != server_name)
        .then(|| qualified_tool(raw, &normalized, true))
        .flatten()
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
    serde_json::from_str::<Value>(raw_input).map_or(GatewayToolIdentity::NotGateway, |value| {
        gateway_tool_identity(&value, 0)
    })
}

fn qualified_tool(raw: &str, server_name: &str, normalized: bool) -> Option<GatewayRoute> {
    let mcp_prefix = format!("mcp__{server_name}__");
    if let Some(tool) = raw.strip_prefix(&mcp_prefix).and_then(bare_gateway_tool) {
        return Some(GatewayRoute {
            tool,
            form: if normalized {
                GatewayNameForm::McpNormalized
            } else {
                GatewayNameForm::McpQualified
            },
        });
    }
    let host_prefix = format!("{server_name}__");
    raw.strip_prefix(&host_prefix)
        .and_then(bare_gateway_tool)
        .map(|tool| GatewayRoute {
            tool,
            form: if normalized {
                GatewayNameForm::HostNormalized
            } else {
                GatewayNameForm::HostQualified
            },
        })
}

fn normalize_server_name(server_name: &str) -> String {
    server_name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn bare_gateway_tool(name: &str) -> Option<GatewayTool> {
    match name {
        SEARCH_TOOL => Some(GatewayTool::Search),
        READ_TOOL => Some(GatewayTool::Read),
        INVOKE_TOOL => Some(GatewayTool::Invoke),
        IMAGE_TOOL => Some(GatewayTool::Image),
        KNOWLEDGE_TOOL => Some(GatewayTool::Knowledge),
        MEMORY_TOOL => Some(GatewayTool::Memory),
        _ => None,
    }
}

fn canonical_tool(raw: &str) -> Option<GatewayTool> {
    bare_gateway_tool(raw.rsplit("__").next().unwrap_or(raw))
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
    if canonical_tool(tool_name.trim()) != Some(GatewayTool::Invoke) {
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
    let Ok(params) = serde_json::from_value::<IdentityInvokeParams>(value.clone()) else {
        return GatewayToolIdentity::NotGateway;
    };
    let capability_id = params.capability_id.trim();
    if capability_id.is_empty() || capability_id.chars().count() > CAPABILITY_ID_MAX_CHARS {
        return GatewayToolIdentity::NotGateway;
    }
    tool_name_for_capability_id(capability_id)
        .map_or(GatewayToolIdentity::UnresolvedGateway, |tool_name| {
            GatewayToolIdentity::Resolved(tool_name.to_string())
        })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityInvokeParams {
    capability_id: String,
    #[serde(rename = "arguments")]
    _arguments: Map<String, Value>,
    #[serde(rename = "delivery_ack")]
    _delivery_ack: Option<String>,
}
