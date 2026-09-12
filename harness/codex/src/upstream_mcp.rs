use sacp::schema::McpServer;
use serde_json::{json, Map, Value};

use super::{UpstreamClient, UpstreamError};
use crate::{Capability, CapabilitySet};

const MCP_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const MCP_READY_POLL: std::time::Duration = std::time::Duration::from_millis(250);

impl UpstreamClient {
    pub(crate) async fn configure_session_mcp(
        &self,
        params: &Value,
        capabilities: CapabilitySet,
    ) -> Result<(), UpstreamError> {
        let servers: Vec<McpServer> = serde_json::from_value(
            params
                .get("mcpServers")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )
        .map_err(|_| invalid("Invalid ACP MCP server configuration"))?;
        if !servers.is_empty() && !capabilities.contains(Capability::Mcp) {
            return Err(invalid("MCP is not enabled for this Codex runtime"));
        }
        let mut mapped = Map::new();
        for server in servers {
            let (name, config) = map_server(server)?;
            if mapped.insert(name, config).is_some() {
                return Err(invalid("Duplicate ACP MCP server name"));
            }
        }
        eprintln!(
            "[internal-codex-worker] stage=session_mcp status=configured server_count={}",
            mapped.len()
        );
        *self.session_mcp.lock().await = mapped;
        Ok(())
    }

    pub(super) async fn attach_session_mcp(&self, mut request: Value) -> Value {
        let servers = self.session_mcp.lock().await;
        if !servers.is_empty() {
            request["params"]["config"] = json!({"mcp_servers": &*servers});
        }
        request
    }

    pub(crate) async fn verify_session_mcp(&self, thread_id: &str) -> Result<(), UpstreamError> {
        let names = self
            .session_mcp
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if names.is_empty() {
            return Ok(());
        }
        tokio::time::timeout(MCP_READY_TIMEOUT, async {
            loop {
                let catalog = self
                    .send(json!({"method": "mcpServerStatus/list", "params": {
                    "threadId": thread_id, "detail": "toolsAndAuthOnly"
                    }}))
                    .await?;
                if catalog_ready(&catalog, &names)? {
                    return Ok(());
                }
                tokio::time::sleep(MCP_READY_POLL).await;
            }
        })
        .await
        .map_err(|_| {
            invalid("Codex did not load the session MCP tool catalog before the startup deadline")
        })?
    }
}

fn catalog_ready(catalog: &Value, names: &[String]) -> Result<bool, UpstreamError> {
    let servers = catalog
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Codex returned an invalid MCP status response"))?;
    let mut ready = 0;
    for server in servers.iter().filter(|server| {
        server["name"]
            .as_str()
            .is_some_and(|name| names.iter().any(|expected| expected == name))
    }) {
        if let Some(error) = server["toolsError"].as_str() {
            return Err(invalid(&format!(
                "Codex MCP tool discovery failed: {}",
                crate::diagnostics::safe_detail(error)
            )));
        }
        let status = server["runtimeStatus"].as_str().unwrap_or("starting");
        if matches!(
            status,
            "failed" | "cancelled" | "disabled" | "authenticationRequired"
        ) {
            return Err(invalid(&format!("Codex MCP server is not ready: {status}")));
        }
        if status == "connected"
            && server["tools"]
                .as_object()
                .is_some_and(|tools| !tools.is_empty())
        {
            ready += 1;
        }
    }
    if ready == names.len() {
        eprintln!("[internal-codex-worker] stage=session_mcp status=ready server_count={ready}");
        Ok(true)
    } else {
        Ok(false)
    }
}

fn map_server(server: McpServer) -> Result<(String, Value), UpstreamError> {
    let (name, mut config) = match server {
        McpServer::Http(server) => {
            let headers: Map<String, Value> = server
                .headers
                .into_iter()
                .map(|header| (header.name, json!(header.value)))
                .collect();
            (
                server.name,
                json!({"url": server.url, "http_headers": headers}),
            )
        }
        McpServer::Stdio(server) => {
            let env: Map<String, Value> = server
                .env
                .into_iter()
                .map(|entry| (entry.name, json!(entry.value)))
                .collect();
            (
                server.name,
                json!({"command": server.command, "args": server.args, "env": env}),
            )
        }
        _ => {
            return Err(invalid(
                "This Codex runtime does not support ACP SSE MCP servers",
            ))
        }
    };
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid("Invalid ACP MCP server name"));
    }
    config["enabled"] = json!(true);
    config["required"] = json!(true);
    Ok((name, config))
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.to_string())
}
