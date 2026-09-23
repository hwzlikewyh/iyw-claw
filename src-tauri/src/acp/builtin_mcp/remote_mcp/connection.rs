use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{ClientCapabilities, ClientInfo, Implementation, Tool};
use rmcp::service::{Peer, QuitReason, RoleClient};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{ErrorData, ServiceExt};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::failure;

const GATEWAY_URL: &str = "https://gateway.iyw.cn/iyw-fusion-mcp-gateway/gateway/mcp/mcp";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const HTTP_TIMEOUT: Duration = Duration::from_secs(125);
const MAX_TOOL_PAGES: usize = 16;
pub(super) const SEARCH_TOOL: &str = "search_mcp_tools";
pub(super) const READ_TOOL: &str = "read_mcp_tool";
pub(super) const INVOKE_TOOL: &str = "invoke_mcp_tool";

type ServiceTask = JoinHandle<Result<QuitReason, tokio::task::JoinError>>;

pub(super) struct RemoteConnection {
    pub peer: Peer<RoleClient>,
    pub instructions: String,
    pub tools: Vec<Tool>,
    cancellation: CancellationToken,
    task: Mutex<Option<ServiceTask>>,
}

impl RemoteConnection {
    pub(super) async fn open(
        token: &str,
        cancel: &CancellationToken,
    ) -> Result<Arc<Self>, ErrorData> {
        let lifetime = cancel.child_token();
        let guard = lifetime.clone().drop_guard();
        let connection = tokio::time::timeout(CONNECT_TIMEOUT, Self::connect(token, lifetime))
            .await
            .map_err(|_| {
                failure(
                    "remote_initialize_timeout",
                    "Remote MCP initialization timed out",
                    false,
                )
            })??;
        guard.disarm();
        tracing::info!(
            tool_count = connection.tools.len(),
            "[remote-mcp] shared connection ready"
        );
        Ok(Arc::new(connection))
    }

    async fn connect(token: &str, cancellation: CancellationToken) -> Result<Self, ErrorData> {
        let info = ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("iyw-claw-gateway", env!("CARGO_PKG_VERSION")),
        );
        let service = info
            .serve_with_ct(transport(token)?, cancellation.clone())
            .await
            .map_err(|_| {
                failure(
                    "remote_initialize_failed",
                    "Remote MCP initialization failed",
                    false,
                )
            })?;
        let tools = list_tools(&service).await?;
        let instructions = service
            .peer_info()
            .and_then(|info| info.instructions.clone())
            .unwrap_or_default();
        let peer = service.peer().clone();
        let task = tokio::spawn(service.waiting());
        Ok(Self {
            peer,
            instructions,
            tools,
            cancellation,
            task: Mutex::new(Some(task)),
        })
    }

    pub(super) fn is_closed(&self) -> bool {
        self.cancellation.is_cancelled() || self.peer.is_transport_closed()
    }

    pub(super) fn stop(&self) {
        self.cancellation.cancel();
    }

    pub(super) async fn close(&self) -> bool {
        self.stop();
        let mut slot = self.task.lock().await;
        let Some(task) = slot.as_mut() else {
            return true;
        };
        let complete = matches!(task.await, Ok(Ok(_)));
        slot.take();
        if !complete {
            tracing::warn!("[remote-mcp] connection cleanup failed");
        }
        complete
    }
}

impl Drop for RemoteConnection {
    fn drop(&mut self) {
        self.stop();
    }
}

fn transport(token: &str) -> Result<StreamableHttpClientTransport<reqwest_mcp::Client>, ErrorData> {
    let client = reqwest_mcp::Client::builder()
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest_mcp::redirect::Policy::none())
        .build()
        .map_err(|_| {
            failure(
                "remote_client_failed",
                "Cannot create remote MCP client",
                false,
            )
        })?;
    let mut header = reqwest_mcp::header::HeaderValue::from_str(token).map_err(|_| {
        failure(
            "remote_credentials_invalid",
            "Invalid account credentials",
            false,
        )
    })?;
    header.set_sensitive(true);
    let config = StreamableHttpClientTransportConfig::with_uri(GATEWAY_URL)
        .custom_headers(
            [(
                reqwest_mcp::header::HeaderName::from_static("token"),
                header,
            )]
            .into(),
        )
        .reinit_on_expired_session(false);
    Ok(StreamableHttpClientTransport::with_client(client, config))
}

async fn list_tools(peer: &Peer<RoleClient>) -> Result<Vec<Tool>, ErrorData> {
    let mut cursor = None;
    let mut tools = Vec::new();
    for _ in 0..MAX_TOOL_PAGES {
        let page = peer
            .list_tools(Some(rmcp::model::PaginatedRequestParams {
                meta: None,
                cursor,
            }))
            .await
            .map_err(|error| super::request::service_error(error, false))?;
        tools.extend(page.tools);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return validate_directory(tools);
        }
    }
    Err(failure(
        "remote_catalog_invalid",
        "Remote tools/list exceeded the page limit",
        false,
    ))
}

fn validate_directory(tools: Vec<Tool>) -> Result<Vec<Tool>, ErrorData> {
    for name in [SEARCH_TOOL, READ_TOOL, INVOKE_TOOL] {
        if !tools.iter().any(|tool| tool.name == name) {
            return Err(failure(
                "remote_catalog_incompatible",
                "Remote directory is missing a required tool",
                false,
            ));
        }
    }
    Ok(tools)
}
