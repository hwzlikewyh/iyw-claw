use sacp::schema::{HttpHeader, McpServer, McpServerHttp};
use sea_orm::DatabaseConnection;

const SERVER_NAME: &str = "爱原物网关mcp";
const GATEWAY_URL: &str = "https://gateway.iyw.cn/iyw-fusion-mcp-gateway/gateway/mcp/mcp";

pub(super) async fn append(
    servers: &mut Vec<McpServer>,
    db: Option<&DatabaseConnection>,
    http_available: bool,
) {
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
    let server = McpServerHttp::new(SERVER_NAME, GATEWAY_URL)
        .headers(vec![HttpHeader::new("token", token.expose())]);
    servers.push(McpServer::Http(server));
    tracing::info!(
        server_name = SERVER_NAME,
        "[iyw-gateway-mcp] attached remote MCP with current account credentials"
    );
}

fn has_gateway_name(server: &McpServer) -> bool {
    match server {
        McpServer::Http(server) => server.name == SERVER_NAME,
        McpServer::Sse(server) => server.name == SERVER_NAME,
        McpServer::Stdio(server) => server.name == SERVER_NAME,
        _ => false,
    }
}
