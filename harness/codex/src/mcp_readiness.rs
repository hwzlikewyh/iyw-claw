use crate::{UpstreamClient, UpstreamError};
use serde_json::{json, Value};

const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const READY_POLL: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) async fn verify(
    upstream: &UpstreamClient,
    thread: &str,
    names: &[String],
) -> Result<(), UpstreamError> {
    if names.is_empty() {
        return Ok(());
    }
    tokio::time::timeout(READY_TIMEOUT, async {
        loop {
            let catalog = upstream
                .request_global_json(json!({"method": "mcpServerStatus/list", "params": {
                    "threadId": thread, "detail": "toolsAndAuthOnly"
                }}))
                .await?;
            if catalog_ready(&catalog, names)? {
                return Ok(());
            }
            tokio::time::sleep(READY_POLL).await;
        }
    })
    .await
    .map_err(|_| {
        invalid("Codex did not load the session MCP tool catalog before the startup deadline")
    })?
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
        if status == "connected" {
            validate_tools(server)?;
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

fn validate_tools(server: &Value) -> Result<(), UpstreamError> {
    let tools = server["tools"]
        .as_object()
        .ok_or_else(|| invalid("Codex MCP tool catalog is not an object"))?;
    if tools.values().any(|tool| {
        tool["name"]
            .as_str()
            .is_none_or(|name| name.trim().is_empty())
            || !tool["inputSchema"].is_object()
    }) {
        return Err(invalid(
            "Codex MCP catalog lost a concrete tool name or input schema",
        ));
    }
    let name = server["name"].as_str().unwrap_or_default();
    if !name.starts_with("iyw-claw-builtin-") && !name.starts_with("iyw_claw_builtin_") {
        return Ok(());
    }
    for (name, field) in [
        ("search_iyw_capabilities", "query"),
        ("read_iyw_capability", "capability_id"),
        ("invoke_iyw_capability", "arguments"),
    ] {
        let valid = tools.values().any(|tool| {
            tool["name"] == name
                && tool["inputSchema"]["properties"]
                    .as_object()
                    .is_some_and(|properties| properties.contains_key(field))
        });
        if !valid {
            return Err(invalid(&format!(
                "Codex built-in MCP catalog lost {name} or its schema"
            )));
        }
    }
    Ok(())
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.to_string())
}
