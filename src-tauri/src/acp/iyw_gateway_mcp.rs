use sacp::schema::{HttpHeader, McpServer, McpServerHttp};
use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const SERVER_NAME: &str = "爱原物网关mcp";
const GATEWAY_URL: &str = "https://gateway.iyw.cn/iyw-fusion-mcp-gateway/gateway/mcp/mcp";
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const FAILURE_COOLDOWN: Duration = Duration::from_secs(30);
const SUCCESS_CACHE: Duration = Duration::from_secs(5);

fn gateway_probe_state() -> &'static Mutex<Option<([u8; 32], bool, Instant)>> {
    static STATE: OnceLock<Mutex<Option<([u8; 32], bool, Instant)>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

pub(super) async fn append(
    servers: &mut Vec<McpServer>,
    db: Option<&DatabaseConnection>,
    runtime: (crate::models::AgentType, bool),
) {
    let (agent_type, http_available) = runtime;
    if !http_available {
        tracing::debug!("[iyw-gateway-mcp] skipped: HTTP MCP is unavailable for this agent");
        return;
    }
    if servers.iter().any(has_gateway_name) {
        tracing::warn!("[iyw-gateway-mcp] skipped: a server with this name is already configured");
        return;
    }
    let Some(db) = db else {
        tracing::debug!("[iyw-gateway-mcp] skipped: account database is unavailable");
        return;
    };
    let token = match crate::commands::iyw_account::iyw_account_access_token_core(db).await {
        Ok(Some(token)) => token,
        Ok(None) => {
            tracing::info!("[iyw-gateway-mcp] skipped: IYW account is not signed in");
            return;
        }
        Err(error) => {
            tracing::warn!(error = %error, "[iyw-gateway-mcp] failed to load account credentials");
            return;
        }
    };
    // 内置星河把该网关标记为 optional，由原生 MCP 管理器并行连接。
    let probe_required = !crate::internal_xinghe_worker::is_desktop_agent(agent_type);
    if probe_required && !gateway_available(token.expose()).await {
        tracing::debug!(
            "[iyw-gateway-mcp] remote gateway unavailable; continuing session without it"
        );
        return;
    }
    let server = McpServerHttp::new(SERVER_NAME, GATEWAY_URL)
        .headers(vec![HttpHeader::new("token", token.expose())]);
    servers.push(McpServer::Http(server));
    tracing::info!(
        server_name = SERVER_NAME,
        probe_required,
        "[iyw-gateway-mcp] attached remote MCP with current account credentials"
    );
}

async fn gateway_available(token: &str) -> bool {
    match tokio::time::timeout(PROBE_TIMEOUT, cached_probe(token)).await {
        Ok(ready) => ready,
        Err(_) => {
            if let Ok(mut state) = gateway_probe_state().try_lock() {
                *state = Some((
                    Sha256::digest(token.as_bytes()).into(),
                    false,
                    Instant::now() + FAILURE_COOLDOWN,
                ));
            }
            false
        }
    }
}

async fn cached_probe(token: &str) -> bool {
    let mut state = gateway_probe_state().lock().await;
    let fingerprint: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    if let Some((key, ready, until)) = *state {
        if key == fingerprint && until > Instant::now() {
            return ready;
        }
    }
    let ready = probe_gateway(token).await;
    if ready {
        if state.is_some_and(|(_, old_ready, _)| !old_ready) {
            tracing::info!("[iyw-gateway-mcp] remote gateway recovered");
        }
    } else {
        if state.is_none_or(|(_, old_ready, _)| old_ready) {
            tracing::warn!(
                "[iyw-gateway-mcp] remote gateway probe failed; temporarily suspending probes"
            );
        }
    }
    let ttl = if ready {
        SUCCESS_CACHE
    } else {
        FAILURE_COOLDOWN
    };
    *state = Some((fingerprint, ready, Instant::now() + ttl));
    ready
}

async fn probe_gateway(token: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    let response = client
        .post(GATEWAY_URL)
        .header("token", token)
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "iyw-claw-health", "version": "1"}}
        }))
        .send()
        .await;
    let Ok(response) = response else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let session = response.headers().get("Mcp-Session-Id").cloned();
    let valid = tokio::time::timeout(Duration::from_secs(1), read_probe_response(response))
        .await
        .unwrap_or(false);
    if let Some(session) = session {
        let cleanup = client.delete(GATEWAY_URL)
            .header("token", token).header("Mcp-Session-Id", session);
        // 探测已结束，回收临时远端会话不再占用 Agent 启动的关键路径。
        tokio::spawn(async move {
            if let Err(error) = cleanup.send().await {
                tracing::debug!(timeout = error.is_timeout(),
                    "[iyw-gateway-mcp] probe session cleanup failed");
            }
        });
    }
    valid
}

async fn read_probe_response(response: reqwest::Response) -> bool {
    use futures_util::StreamExt;
    const MAX_PROBE_BYTES: usize = 64 * 1024;
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return false;
        };
        if buffer.len() + chunk.len() > MAX_PROBE_BYTES {
            return false;
        }
        buffer.extend_from_slice(&chunk);
        let text = String::from_utf8_lossy(&buffer);
        if probe_payload_valid(&text) {
            return true;
        }
    }
    false
}

fn probe_payload_valid(text: &str) -> bool {
    let valid = |value: &str| {
        serde_json::from_str::<serde_json::Value>(value)
            .ok()
            .is_some_and(|json| {
                json.get("result")
                    .and_then(|r| r.get("protocolVersion"))
                    .is_some()
            })
    };
    valid(text)
        || text
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .any(|line| valid(line.trim()))
}

fn has_gateway_name(server: &McpServer) -> bool {
    match server {
        McpServer::Http(server) => server.name == SERVER_NAME,
        McpServer::Sse(server) => server.name == SERVER_NAME,
        McpServer::Stdio(server) => server.name == SERVER_NAME,
        _ => false,
    }
}
