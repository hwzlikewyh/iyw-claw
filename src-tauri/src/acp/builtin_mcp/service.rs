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
use super::iyw_service::IywGatewayService;
use super::lease::{BuiltinMcpIssueError, LeaseManager, LeaseShutdownReport};

mod shutdown;
mod tasks;

use tasks::spawn_tasks;

pub(super) const SERVER_INSTRUCTIONS: &str = "Choose the call surface before constructing arguments. A directly advertised tool is called by its exact visible identity with its own schema; reading that definition needs no read_iyw_capability call. generate_iyw_image owns image production directly and has no image-generation capability_id. Catalog capability IDs are opaque values: copy them from this session's search result or the advertised manage_iyw_memory operation mapping, never derive them from tool names, descriptions, or naming patterns. Before invoke_iyw_capability or a memory operation, read that exact capability's full instructions/schema once and reuse the read; search summaries are insufficient. Use only declared business fields in the documented arguments or parameters object. The host does not record read history or add a read gate. Distinguish failures: capability_not_found is a rejected ID lookup; follow its evidence-based hint without guessing more IDs. A backend rejection or timeout is not a discovery error and must not trigger a renamed or alternate-tool replay. If the server cannot resolve the advertised gateway tool identity itself, stop using this server for the turn; do not try bare, prefixed, normalized, or foreign names. A lookup's execution_status=not_started says nothing about earlier business calls. Use only the host's actual callable namespace or registry.";

#[derive(Clone)]
pub struct BuiltinMcpClient {
    endpoint: Arc<str>,
    ready: Arc<AtomicBool>,
    leases: Arc<LeaseManager>,
    advertised_tools: Arc<[String]>,
    capability_tools: Arc<[String]>,
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

    pub fn capability_tools(&self) -> &[String] {
        &self.capability_tools
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

    pub(crate) async fn reset_transport(&self, connection_id: &str) -> Result<(), String> {
        self.leases.reset_transport(connection_id).await
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
    pub async fn start(
        listener: Arc<DelegationListener>,
        db: sea_orm::DatabaseConnection,
    ) -> io::Result<Arc<Self>> {
        super::capability::CapabilityCatalog::load()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let advertised_tools = gateway_tool_names()?;
        let capability_tools = embedded_tool_names()?;
        let iyw = IywGatewayService::new(db, listener.clone())
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;
        let tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        crate::web::socket_inherit::mark_listener_non_inheritable(&tcp)?;
        let authority = tcp.local_addr()?.to_string();
        let endpoint: Arc<str> = format!("http://{authority}/mcp").into();
        let shutdown = CancellationToken::new();
        let (router, client) = build_runtime(
            listener,
            authority,
            endpoint,
            advertised_tools,
            capability_tools,
            iyw,
            &shutdown,
        );
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
    capability_tools: Arc<[String]>,
    iyw: Arc<IywGatewayService>,
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
                Arc::clone(&iyw),
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
        capability_tools,
    };
    (router, client)
}

fn gateway_tool_names() -> io::Result<Arc<[String]>> {
    let tools = super::gateway::tools()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let names = tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    if names.len() != super::gateway_tools::values().len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP MCP gateway tool identities are inconsistent",
        ));
    }
    Ok(names.into())
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
