use serde_json::{json, Map, Value};

use crate::{Capability, CapabilitySet, UpstreamError};

/// 只携带桥接层已校验的 MCP 覆盖，不开放任意线程配置覆盖。
#[derive(Clone)]
pub(crate) struct ThreadLaunchOptions {
    pub capabilities: CapabilitySet,
    mcp: Option<Map<String, Value>>,
    fork_settings: Option<Value>,
}

impl ThreadLaunchOptions {
    pub(crate) fn mcp_names(&self) -> Vec<String> {
        self.mcp.as_ref().map(|servers| servers.keys().cloned().collect()).unwrap_or_default()
    }
    pub(crate) fn new(capabilities: CapabilitySet) -> Self {
        Self {
            capabilities,
            mcp: None,
            fork_settings: None,
        }
    }

    pub(crate) fn from_acp(
        params: &Value,
        capabilities: CapabilitySet,
    ) -> Result<Self, UpstreamError> {
        let Some(servers) = params.get("mcpServers") else {
            return Ok(Self::new(capabilities));
        };
        let servers = servers
            .as_array()
            .ok_or_else(|| invalid("mcpServers must be an array"))?;
        if servers.is_empty() {
            return Ok(Self::new(capabilities));
        }
        if !capabilities.contains(Capability::Mcp) {
            return Err(invalid("MCP is not enabled for this session"));
        }
        let mut mapped = Map::new();
        for server in servers {
            let name = server_name(required_string(server, "name")?);
            let config = server_config(server)?;
            if mapped.insert(name, config).is_some() {
                return Err(invalid("MCP names collide after normalization"));
            }
        }
        Ok(Self {
            capabilities,
            mcp: Some(mapped),
            fork_settings: None,
        })
    }

    pub(crate) fn apply(self, request: &mut Value) {
        if let Some(servers) = self.mcp {
            request["params"]["config"] = json!({ "mcp_servers": servers });
        }
        if let Some(settings) = self.fork_settings {
            request["params"]["model"] = settings["model"].clone();
            request["params"]["serviceTier"] = settings["serviceTier"].clone();
            if !request["params"]["config"].is_object() {
                request["params"]["config"] = json!({});
            }
            if !settings["effort"].is_null() {
                request["params"]["config"]["model_reasoning_effort"] = settings["effort"].clone();
            }
        }
    }

    pub(crate) fn with_fork_settings(mut self, settings: Value) -> Result<Self, UpstreamError> {
        let fields = settings
            .as_object()
            .ok_or_else(|| invalid("fork settings must be an object"))?;
        if fields.keys().any(|field| {
            !matches!(
                field.as_str(),
                "model" | "effort" | "serviceTier" | "collaborationMode"
            )
        }) {
            return Err(invalid("fork settings contain unmanaged overrides"));
        }
        self.fork_settings = Some(settings);
        Ok(self)
    }

    pub(crate) fn fork_settings(&self) -> Option<Value> {
        self.fork_settings.clone()
    }
}

fn server_config(server: &Value) -> Result<Value, UpstreamError> {
    let mut config = match server.get("type").and_then(Value::as_str) {
        Some("http") => json!({
            "url": required_string(server, "url")?,
            "http_headers": named_values(server, "headers")?,
        }),
        None | Some("stdio") => json!({
            "command": required_string(server, "command")?,
            "args": string_array(server, "args")?,
            "env": named_values(server, "env")?,
        }),
        _ => {
            return Err(invalid(
                "MCP transport is not supported by the locked runtime",
            ))
        }
    };
    config["enabled"] = json!(true);
    config["required"] = json!(true);
    // 上游校验的错误只返回类型说明，避免输出命令、参数或认证头。
    serde_json::from_value::<codex_config::McpServerConfig>(config.clone())
        .map_err(|_| invalid("MCP configuration failed runtime validation"))?;
    Ok(config)
}

fn named_values(server: &Value, field: &str) -> Result<Map<String, Value>, UpstreamError> {
    let Some(values) = server.get(field) else {
        return Ok(Map::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| invalid("MCP named values must be an array"))?;
    let mut mapped = Map::new();
    for item in values {
        let name = required_string(item, "name")?;
        let value = item
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("MCP named value must be a string"))?;
        if mapped.insert(name.into(), json!(value)).is_some() {
            return Err(invalid("MCP named values contain a duplicate name"));
        }
    }
    Ok(mapped)
}

fn string_array(server: &Value, field: &str) -> Result<Value, UpstreamError> {
    let Some(values) = server.get(field) else {
        return Ok(json!([]));
    };
    if !values
        .as_array()
        .is_some_and(|items| items.iter().all(Value::is_string))
    {
        return Err(invalid("MCP arguments must be strings"));
    }
    Ok(values.clone())
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, UpstreamError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid("MCP configuration is missing a required string"))
}

fn server_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.into())
}
