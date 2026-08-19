use std::io;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::{middleware, Router};
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::listener::DelegationListener;
use crate::acp::memory_turn::MemoryTurnTracker;

use super::authority::SessionAuthority;
use super::credential::SessionToken;
use super::handler::BuiltinMcpHandler;
use super::http::{authenticate_request, AuthHttpState};
use super::lease::{BuiltinMcpIssueError, LeaseManager, LeaseShutdownReport};

mod shutdown;
mod tasks;

use tasks::spawn_tasks;

#[derive(Clone)]
pub struct BuiltinMcpClient {
    endpoint: Arc<str>,
    ready: Arc<AtomicBool>,
    leases: Arc<LeaseManager>,
    advertised_tools: Arc<[String]>,
}

impl BuiltinMcpClient {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn advertised_tools(&self) -> &[String] {
        &self.advertised_tools
    }

    pub async fn issue(
        &self,
        authority: SessionAuthority,
        turn_tracker: Arc<MemoryTurnTracker>,
    ) -> Result<SessionToken, BuiltinMcpIssueError> {
        if !self.is_ready() {
            authority.cancellation().cancel();
            return Err(BuiltinMcpIssueError::ServiceUnavailable);
        }
        self.leases
            .issue(authority, turn_tracker, self.ready.as_ref())
            .await
    }

    pub async fn revoke_parent(&self, connection_id: &str) -> usize {
        self.leases.revoke_parent(connection_id).await
    }

    async fn revoke_all(&self) -> LeaseShutdownReport {
        self.leases.revoke_all().await
    }

    async fn begin_revoke_all(&self) -> (usize, bool) {
        self.leases.begin_revoke_all().await
    }
}

pub struct BuiltinMcpService {
    client: BuiltinMcpClient,
    shutdown: CancellationToken,
    joins: Mutex<Vec<JoinHandle<()>>>,
}

impl BuiltinMcpService {
    pub async fn start(listener: Arc<DelegationListener>) -> io::Result<Arc<Self>> {
        super::capability::CapabilityCatalog::load()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let advertised_tools = embedded_tool_names()?;
        let tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        crate::web::socket_inherit::mark_listener_non_inheritable(&tcp)?;
        let authority = tcp.local_addr()?.to_string();
        let endpoint: Arc<str> = format!("http://{authority}/mcp").into();
        let shutdown = CancellationToken::new();
        let (router, client) =
            build_runtime(listener, authority, endpoint, advertised_tools, &shutdown);
        let joins = spawn_tasks(tcp, router, client.clone(), shutdown.clone());
        tracing::info!(
            target: "builtin_mcp",
            endpoint = %client.endpoint(),
            "process HTTP MCP service ready"
        );
        Ok(Arc::new(Self {
            client,
            shutdown,
            joins: Mutex::new(joins),
        }))
    }

    pub fn client(&self) -> BuiltinMcpClient {
        self.client.clone()
    }

    pub fn quiesce(&self) {
        self.client.ready.store(false, Ordering::Release);
    }
}

fn build_runtime(
    listener: Arc<DelegationListener>,
    authority: String,
    endpoint: Arc<str>,
    advertised_tools: Arc<[String]>,
    shutdown: &CancellationToken,
) -> (Router, BuiltinMcpClient) {
    let leases = LeaseManager::new(Arc::clone(&listener));
    let handler_runtimes = leases.runtimes();
    let handler_receipts = leases.receipts();
    let handler_lifecycle = leases.lifecycle();
    let protocol_sessions = leases.protocol_sessions();
    let protocol = StreamableHttpService::new(
        move || {
            Ok(BuiltinMcpHandler::new(
                Arc::clone(&listener),
                Arc::clone(&handler_runtimes),
                handler_receipts.clone(),
                Arc::clone(&handler_lifecycle),
            ))
        },
        Arc::clone(&protocol_sessions),
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown.child_token()),
    );
    let ready = Arc::new(AtomicBool::new(true));
    let auth = AuthHttpState::new(
        authority,
        leases.sessions(),
        leases.bindings(),
        protocol_sessions,
        Arc::clone(&ready),
    );
    let router = Router::new()
        .route_service("/mcp", protocol)
        .layer(middleware::from_fn_with_state(auth, authenticate_request));
    let client = BuiltinMcpClient {
        endpoint,
        ready,
        leases,
        advertised_tools,
    };
    (router, client)
}

fn embedded_tool_names() -> io::Result<Arc<[String]>> {
    let catalog = crate::acp::delegation::companion::filtered_tools(
        crate::acp::delegation::companion::CompanionFeatures::all_enabled(),
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let tools = catalog.as_array().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "embedded MCP tool catalog is not an array",
        )
    })?;
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if names.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "embedded MCP tool catalog is empty",
        ));
    }
    Ok(names.into())
}
