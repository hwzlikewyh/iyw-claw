//! Upstream iyw platform MCP forwarding — the main-process half of the
//! `iyw-platform` companion instance.
//!
//! The platform (`ai-application` behind the iyw gateway) exposes a standard
//! MCP streamable-HTTP endpoint. iyw-claw relays it to agent CLIs through the
//! same stdio companion binary used for delegation (`iyw-claw-mcp --features
//! platform`): the companion forwards `tools/list` / `tools/call` over the
//! delegation UDS broker, and THIS module performs the actual HTTP calls,
//! attaching the logged-in platform account's access token. The token never
//! leaves the main process — it is not passed to the companion, the agent
//! CLI, or any config file. See `docs/iyw-mcp-rust-forwarding.md`.
//!
//! Upstream protocol notes:
//! * One shared upstream MCP session per iyw-claw process (`initialize` →
//!   `Mcp-Session-Id` response header → `notifications/initialized`), lazily
//!   established and rebuilt once when the upstream reports 404 (session
//!   expired / server restarted). The upstream tools are stateless business
//!   APIs, so per-agent-session isolation is unnecessary (confirmed with the
//!   platform team).
//! * A POST response may be encoded as plain JSON or as an SSE stream
//!   (`text/event-stream`) — both are valid streamable-HTTP server choices, so
//!   both are parsed. The upstream pushes no server-initiated notifications
//!   (confirmed), so no GET stream is opened.
//! * The auth header is `token: <access_token>` (bare token, same convention
//!   as every other gateway call — see `commands::iyw_account`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

pub const PLATFORM_MCP_LOCAL_URL: &str = "http://127.0.0.1:5002/mcp";
pub const PLATFORM_MCP_TEST_URL: &str = "http://192.168.1.86:3201/ai-application/mcp";
pub const PLATFORM_MCP_PRODUCTION_URL: &str = "https://gateway.iyw.cn/ai-application/mcp";

// Same three-way selection as `provider_overlay::MODEL_GATEWAY_BASE_URL`:
// debug builds target the local dev service, `test-gateway` release builds the
// staging gateway, everything else production.
#[cfg(debug_assertions)]
const PLATFORM_MCP_DEFAULT_URL: &str = PLATFORM_MCP_LOCAL_URL;
#[cfg(all(not(debug_assertions), feature = "test-gateway"))]
const PLATFORM_MCP_DEFAULT_URL: &str = PLATFORM_MCP_TEST_URL;
#[cfg(all(not(debug_assertions), not(feature = "test-gateway")))]
const PLATFORM_MCP_DEFAULT_URL: &str = PLATFORM_MCP_PRODUCTION_URL;

/// Environment override for the upstream MCP URL — mirrors
/// `IYW_CLAW_MODEL_GATEWAY_BASE_URL` for private / staging deployments.
pub const PLATFORM_MCP_URL_ENV: &str = "IYW_CLAW_PLATFORM_MCP_URL";

pub fn platform_mcp_url() -> String {
    std::env::var(PLATFORM_MCP_URL_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| PLATFORM_MCP_DEFAULT_URL.to_string())
}

/// Agent-facing error when no platform account session exists. Injection
/// already skips the `iyw-platform` entry for logged-out sessions, so this is
/// only reachable when the user logs out mid-session.
pub const PLATFORM_NOT_LOGGED_IN: &str =
    "iyw platform account is not logged in — sign in from iyw-claw, then retry.";

/// Agent-facing error when the upstream rejects the stored token.
pub const PLATFORM_LOGIN_EXPIRED: &str =
    "iyw platform login expired — sign in again from iyw-claw, then retry.";

const INIT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// Agents call `tools/list` at every session start; cache briefly so a burst
/// of session launches doesn't hammer the gateway.
const TOOLS_CACHE_TTL: Duration = Duration::from_secs(60);

/// The protocol revision we offer at `initialize`. The upstream may negotiate
/// down; whatever it returns is echoed back via `mcp-protocol-version`.
const OFFERED_PROTOCOL_VERSION: &str = "2025-03-26";

/// Feature toggle read at MCP injection time, hot-swappable like
/// [`crate::acp::session_info::SessionInfoRuntimeConfig`]. Defaults to
/// enabled — the effective launch gate is the login state; this exists so a
/// future Settings toggle can switch the forwarder off without a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformToolsConfig {
    pub enabled: bool,
}

impl Default for PlatformToolsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Default)]
pub struct PlatformToolsRuntimeConfig {
    inner: Arc<RwLock<PlatformToolsConfig>>,
}

impl PlatformToolsRuntimeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn snapshot(&self) -> PlatformToolsConfig {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, cfg: PlatformToolsConfig) {
        *self.inner.write().await = cfg;
    }

    pub async fn is_enabled(&self) -> bool {
        self.inner.read().await.enabled
    }
}

/// Source of the platform account access token. Production reads the stored
/// login session from the DB on every request (so re-login / account switch
/// takes effect immediately); tests inject a static value.
#[async_trait]
pub trait AccessTokenProvider: Send + Sync {
    async fn access_token(&self) -> Option<String>;
}

pub struct DbAccessTokenProvider {
    pub conn: DatabaseConnection,
}

#[async_trait]
impl AccessTokenProvider for DbAccessTokenProvider {
    async fn access_token(&self) -> Option<String> {
        crate::commands::iyw_account::iyw_account_access_token_core(&self.conn)
            .await
            .ok()
            .flatten()
            .map(|token| token.expose().to_string())
    }
}

/// Listener-facing forwarding surface. Mirrors
/// [`crate::acp::session_info::SessionInfoAccess`]: the production impl is
/// [`PlatformMcpService`], tests use in-memory stubs.
#[async_trait]
pub trait PlatformMcpAccess: Send + Sync {
    /// Fetch the upstream tool catalog. `Ok` is the JSON array from the
    /// upstream `tools/list` result (possibly empty).
    async fn list_tools(&self) -> Result<Value, String>;

    /// Forward one `tools/call`. `Ok` is the upstream MCP result object
    /// verbatim (`content` / `structuredContent` / `isError` shaped).
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String>;
}

#[derive(Clone)]
struct UpstreamSession {
    session_id: Option<String>,
    protocol_version: String,
}

struct ToolsCacheEntry {
    fetched_at: Instant,
    tools: Value,
}

pub struct PlatformMcpService {
    http: reqwest::Client,
    base_url: String,
    login: Arc<dyn AccessTokenProvider>,
    session: Mutex<Option<UpstreamSession>>,
    tools_cache: Mutex<Option<ToolsCacheEntry>>,
    next_id: AtomicU64,
}

impl PlatformMcpService {
    pub fn new(login: Arc<dyn AccessTokenProvider>) -> Self {
        Self::with_base_url(platform_mcp_url(), login)
    }

    pub fn with_base_url(base_url: String, login: Arc<dyn AccessTokenProvider>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .user_agent("iyw-claw")
            .build()
            .unwrap_or_default();
        Self {
            http,
            base_url,
            login,
            session: Mutex::new(None),
            tools_cache: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    async fn require_token(&self) -> Result<String, String> {
        self.login
            .access_token()
            .await
            .ok_or_else(|| PLATFORM_NOT_LOGGED_IN.to_string())
    }

    fn post(&self, token: &str, timeout: Duration) -> reqwest::RequestBuilder {
        self.http
            .post(&self.base_url)
            .timeout(timeout)
            .header("Accept", "application/json, text/event-stream")
            .header("token", token)
    }

    /// Return the shared upstream session, performing the `initialize`
    /// handshake on first use. The lock is held across the handshake so
    /// concurrent first calls don't race two sessions into existence.
    async fn ensure_session(&self, token: &str) -> Result<UpstreamSession, String> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(session.clone());
        }
        let session = self.initialize_upstream(token).await?;
        *guard = Some(session.clone());
        Ok(session)
    }

    async fn initialize_upstream(&self, token: &str) -> Result<UpstreamSession, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": OFFERED_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "iyw-claw",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            },
        });
        let resp = self
            .post(token, INIT_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("platform MCP initialize failed: {e}"))?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(PLATFORM_LOGIN_EXPIRED.to_string());
        }
        if !status.is_success() {
            return Err(format!("platform MCP initialize failed: HTTP {status}"));
        }
        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let payload = read_rpc_payload(resp, id).await?;
        let result = rpc_result(payload)?;
        let protocol_version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(OFFERED_PROTOCOL_VERSION)
            .to_string();

        // Handshake completion notification. Best-effort: a lost notification
        // surfaces as a failed follow-up request, which the 404-retry path
        // repairs by re-initializing.
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        let mut req = self
            .post(token, INIT_TIMEOUT)
            .header("mcp-protocol-version", &protocol_version);
        if let Some(sid) = &session_id {
            req = req.header("mcp-session-id", sid);
        }
        let _ = req.json(&note).send().await;

        Ok(UpstreamSession {
            session_id,
            protocol_version,
        })
    }

    /// One JSON-RPC request → result round-trip, with a single retry through a
    /// fresh handshake when the upstream reports the session gone (404).
    async fn rpc_round_trip(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let token = self.require_token().await?;
        let mut retried = false;
        loop {
            let session = self.ensure_session(&token).await?;
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let body = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params.clone(),
            });
            let mut req = self
                .post(&token, timeout)
                .header("mcp-protocol-version", &session.protocol_version);
            if let Some(sid) = &session.session_id {
                req = req.header("mcp-session-id", sid);
            }
            let resp = req
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("platform MCP request failed: {e}"))?;
            let status = resp.status();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                // Token no longer accepted: drop the session so a post-re-login
                // call starts from a clean handshake.
                self.session.lock().await.take();
                return Err(PLATFORM_LOGIN_EXPIRED.to_string());
            }
            if status.as_u16() == 404 && !retried {
                // Session expired / upstream restarted: rebuild once.
                self.session.lock().await.take();
                retried = true;
                continue;
            }
            if !status.is_success() {
                return Err(format!("platform MCP request failed: HTTP {status}"));
            }
            let payload = read_rpc_payload(resp, id).await?;
            return rpc_result(payload);
        }
    }
}

#[async_trait]
impl PlatformMcpAccess for PlatformMcpService {
    async fn list_tools(&self) -> Result<Value, String> {
        {
            let cache = self.tools_cache.lock().await;
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < TOOLS_CACHE_TTL {
                    return Ok(entry.tools.clone());
                }
            }
        }
        let result = self
            .rpc_round_trip("tools/list", json!({}), LIST_TIMEOUT)
            .await?;
        let tools = result
            .get("tools")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        *self.tools_cache.lock().await = Some(ToolsCacheEntry {
            fetched_at: Instant::now(),
            tools: tools.clone(),
        });
        Ok(tools)
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
        let params = json!({ "name": name, "arguments": arguments });
        self.rpc_round_trip("tools/call", params, CALL_TIMEOUT).await
    }
}

/// Decode a streamable-HTTP POST response body into the JSON-RPC payload for
/// `id`. Handles both server encodings: plain `application/json` and an SSE
/// stream whose `data:` events carry JSON-RPC messages (the response possibly
/// preceded by notifications we ignore).
async fn read_rpc_payload(resp: reqwest::Response, id: u64) -> Result<Value, String> {
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("platform MCP response read failed: {e}"))?;
    if content_type.contains("text/event-stream") {
        sse_response_payload(&body, id)
            .ok_or_else(|| "platform MCP SSE stream ended without a response".to_string())
    } else {
        serde_json::from_str(&body)
            .map_err(|e| format!("platform MCP response is not valid JSON: {e}"))
    }
}

/// Extract the JSON-RPC response for `id` from an SSE body: split into events
/// (blank-line delimited), join each event's `data:` lines, and pick the
/// response-shaped payload matching `id` — falling back to the only response
/// present when the server echoes a different id shape.
fn sse_response_payload(body: &str, id: u64) -> Option<Value> {
    let mut responses: Vec<Value> = Vec::new();
    let mut data = String::new();
    let flush = |data: &mut String, responses: &mut Vec<Value>| {
        if data.is_empty() {
            return;
        }
        if let Ok(payload) = serde_json::from_str::<Value>(data) {
            let is_response = payload.get("id").is_some()
                && (payload.get("result").is_some() || payload.get("error").is_some());
            if is_response {
                responses.push(payload);
            }
        }
        data.clear();
    };
    for line in body.lines() {
        if line.is_empty() {
            flush(&mut data, &mut responses);
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim_start());
        }
    }
    flush(&mut data, &mut responses);

    if let Some(matched) = responses.iter().find(|p| p.get("id") == Some(&json!(id))) {
        return Some(matched.clone());
    }
    if responses.len() == 1 {
        return responses.pop();
    }
    None
}

/// Map a JSON-RPC payload to its `result`, folding a JSON-RPC `error` object
/// into the transport error string the companion renders as a tool error.
fn rpc_result(payload: Value) -> Result<Value, String> {
    if let Some(error) = payload.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
        return Err(format!("platform MCP error {code}: {message}"));
    }
    payload
        .get("result")
        .cloned()
        .ok_or_else(|| "platform MCP response missing result".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::{IntoResponse, Response};
    use std::sync::atomic::AtomicBool;

    struct StaticToken(Option<&'static str>);

    #[async_trait]
    impl AccessTokenProvider for StaticToken {
        async fn access_token(&self) -> Option<String> {
            self.0.map(str::to_string)
        }
    }

    #[derive(Default)]
    struct MockBehavior {
        reject_auth: AtomicBool,
        drop_session_once: AtomicBool,
        initialize_count: AtomicU64,
        list_count: AtomicU64,
        seen_tokens: Mutex<Vec<String>>,
    }

    async fn mock_handler(
        State(behavior): State<Arc<MockBehavior>>,
        headers: HeaderMap,
        body: String,
    ) -> Response {
        if let Some(token) = headers.get("token").and_then(|v| v.to_str().ok()) {
            behavior.seen_tokens.lock().await.push(token.to_string());
        }
        if behavior.reject_auth.load(Ordering::Relaxed) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let payload: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let id = payload.get("id").cloned().unwrap_or(Value::Null);
        match payload.get("method").and_then(Value::as_str) {
            Some("initialize") => {
                behavior.initialize_count.fetch_add(1, Ordering::Relaxed);
                let result = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "mock-platform", "version": "0.0.0" },
                    }
                });
                (
                    [("mcp-session-id", "sess-1"), ("content-type", "application/json")],
                    result.to_string(),
                )
                    .into_response()
            }
            Some("notifications/initialized") => StatusCode::ACCEPTED.into_response(),
            Some("tools/list") => {
                if behavior.drop_session_once.swap(false, Ordering::Relaxed) {
                    return StatusCode::NOT_FOUND.into_response();
                }
                behavior.list_count.fetch_add(1, Ordering::Relaxed);
                let result = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": [ { "name": "echo", "inputSchema": { "type": "object" } } ] }
                });
                ([("content-type", "application/json")], result.to_string()).into_response()
            }
            Some("tools/call") => {
                // Respond in SSE form to exercise the dual-encoding parser: a
                // notification event first, then the response event.
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [ { "type": "text", "text": "echoed" } ],
                        "isError": false
                    }
                });
                let sse = format!(
                    "event: message\ndata: {}\n\nevent: message\ndata: {}\n\n",
                    json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": {} }),
                    response
                );
                ([("content-type", "text/event-stream")], sse).into_response()
            }
            _ => StatusCode::BAD_REQUEST.into_response(),
        }
    }

    async fn spawn_mock(behavior: Arc<MockBehavior>) -> String {
        let app = axum::Router::new()
            .route("/mcp", axum::routing::post(mock_handler))
            .with_state(behavior);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}/mcp")
    }

    fn service(url: String, token: Option<&'static str>) -> PlatformMcpService {
        PlatformMcpService::with_base_url(url, Arc::new(StaticToken(token)))
    }

    #[tokio::test]
    async fn happy_path_list_and_call_with_token_header() {
        let behavior = Arc::new(MockBehavior::default());
        let url = spawn_mock(behavior.clone()).await;
        let svc = service(url, Some("tok-1"));

        let tools = svc.list_tools().await.expect("list should succeed");
        assert_eq!(tools[0]["name"], "echo");

        let result = svc
            .call_tool("echo", json!({ "text": "hi" }))
            .await
            .expect("call should succeed");
        assert_eq!(result["content"][0]["text"], "echoed");
        assert_eq!(result["isError"], false);

        // Every upstream request carried the bare platform token.
        let tokens = behavior.seen_tokens.lock().await;
        assert!(!tokens.is_empty());
        assert!(tokens.iter().all(|t| t == "tok-1"));
    }

    #[tokio::test]
    async fn tools_list_is_cached() {
        let behavior = Arc::new(MockBehavior::default());
        let url = spawn_mock(behavior.clone()).await;
        let svc = service(url, Some("tok-1"));

        svc.list_tools().await.expect("first list");
        svc.list_tools().await.expect("second list");
        assert_eq!(behavior.list_count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn session_rebuilt_after_upstream_404() {
        let behavior = Arc::new(MockBehavior::default());
        behavior.drop_session_once.store(true, Ordering::Relaxed);
        let url = spawn_mock(behavior.clone()).await;
        let svc = service(url, Some("tok-1"));

        let tools = svc.list_tools().await.expect("list should recover via re-handshake");
        assert_eq!(tools[0]["name"], "echo");
        assert_eq!(behavior.initialize_count.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn unauthorized_maps_to_login_expired() {
        let behavior = Arc::new(MockBehavior::default());
        behavior.reject_auth.store(true, Ordering::Relaxed);
        let url = spawn_mock(behavior.clone()).await;
        let svc = service(url, Some("tok-1"));

        let err = svc.list_tools().await.expect_err("401 should fail");
        assert_eq!(err, PLATFORM_LOGIN_EXPIRED);
    }

    #[tokio::test]
    async fn missing_login_short_circuits_without_http() {
        let svc = service("http://127.0.0.1:9/mcp".to_string(), None);
        let err = svc.list_tools().await.expect_err("no token should fail");
        assert_eq!(err, PLATFORM_NOT_LOGGED_IN);
    }

    #[test]
    fn sse_payload_picks_response_matching_id() {
        let body = concat!(
            "event: message\n",
            "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n",
            "\n",
            "data: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"ok\":true}}\n",
            "\n",
        );
        let payload = sse_response_payload(body, 7).expect("should find id 7");
        assert_eq!(payload["result"]["ok"], true);
    }

    #[test]
    fn sse_payload_falls_back_to_sole_response() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":\"weird\",\"result\":{}}\n\n";
        assert!(sse_response_payload(body, 3).is_some());
        assert!(sse_response_payload("data: {\"method\":\"x\"}\n\n", 3).is_none());
    }

    #[test]
    fn rpc_error_folds_into_message() {
        let err = rpc_result(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32000, "message": "boom" }
        }))
        .expect_err("error payload should map to Err");
        assert!(err.contains("-32000") && err.contains("boom"));
    }

    #[test]
    fn default_config_is_enabled() {
        assert!(PlatformToolsConfig::default().enabled);
    }
}
