use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use rmcp::ServiceExt;
use tokio::io::DuplexStream;
use tokio_util::sync::CancellationToken;

use super::authority::SessionContext;
use super::handler::BuiltinMcpHandler;
use super::service::BuiltinMcpClient;

mod tasks;
mod transport;
pub(super) use tasks::Tasks;
pub(super) use transport::RequestContext;

const BUFFER_BYTES: usize = 64 * 1024;

impl BuiltinMcpClient {
    pub(crate) fn in_process_transport(
        &self,
        connection_id: &str,
    ) -> iyw_codex_harness::BuiltinMcpTransport {
        let client = self.clone();
        let owner = Arc::new(Mutex::new(
            (connection_id != "runtime-host-prewarm").then(|| connection_id.to_string()),
        ));
        iyw_codex_harness::BuiltinMcpTransport::new(
            self.endpoint().to_string(),
            move |server, headers| {
                let client = client.clone();
                let owner = Arc::clone(&owner);
                Box::pin(async move { client.open_in_process((server, headers), owner).await })
            },
        )
    }

    async fn open_in_process(
        &self,
        request: (String, HashMap<String, String>),
        owner: Arc<Mutex<Option<String>>>,
    ) -> io::Result<DuplexStream> {
        let (server, headers) = request;
        let lifecycle = self.leases.lifecycle();
        let _lifecycle = lifecycle.lock().await;
        let token = bearer(&headers)?;
        let authority = self
            .leases
            .sessions()
            .lookup(token)
            .await
            .ok_or_else(unauthorized)?;
        if !self.is_ready()
            || authority.agent_type() != crate::models::AgentType::Codex
            || !matches_server(&authority, &server)
        {
            return Err(unauthorized());
        }
        bind_owner(&owner, authority.connection_id())?;
        let handler = self.handler.fresh_connection();
        let (client, stream) = tokio::io::duplex(BUFFER_BYTES);
        let parent = authority.connection_id().to_string();
        self.leases
            .in_process_tasks()
            .spawn(&parent, move |stop| {
                Box::pin(serve(handler, stream, (authority, stop)))
            })
            .await?;
        tracing::info!(target: "builtin_mcp", connection_id = parent, transport = "in_process",
            "opened authorized in-process MCP connection");
        Ok(client)
    }
}

fn bind_owner(owner: &Mutex<Option<String>>, connection_id: &str) -> io::Result<()> {
    let mut owner = owner.lock().map_err(|_| unauthorized())?;
    match owner.as_deref() {
        Some(current) if current != connection_id => Err(unauthorized()),
        Some(_) => Ok(()),
        None => {
            *owner = Some(connection_id.to_string());
            Ok(())
        }
    }
}

fn matches_server(authority: &SessionContext, server: &str) -> bool {
    authority
        .gateway_server_name()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .eq(server.chars())
}

fn bearer(headers: &HashMap<String, String>) -> io::Result<&str> {
    let mut values = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"));
    let (_, value) = values.next().ok_or_else(unauthorized)?;
    if values.next().is_some() {
        return Err(unauthorized());
    }
    let (scheme, token) = value.split_once(' ').ok_or_else(unauthorized)?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err(unauthorized());
    }
    Ok(token)
}

fn unauthorized() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "in-process MCP authority is unavailable",
    )
}

async fn serve(
    handler: BuiltinMcpHandler,
    stream: DuplexStream,
    context: (SessionContext, CancellationToken),
) {
    let (authority, stop) = context;
    let parent = authority.connection_id().to_string();
    let transport = transport::InProcessTransport::new(stream, authority.clone(), stop.clone());
    let service = tokio::select! {
        biased;
        _ = stop.cancelled() => return,
        _ = authority.cancellation().cancelled() => return,
        service = handler.serve_with_ct(transport, stop.clone()) => service,
    };
    match service {
        Ok(service) => {
            // 通道 receive 会在 authority 撤销时结束，waiting 保持任务归属直到清理完成。
            if let Err(error) = service.waiting().await {
                tracing::warn!(target: "builtin_mcp", connection_id = parent, %error, "in-process MCP worker failed");
            }
        }
        Err(_) => {
            tracing::warn!(target: "builtin_mcp", connection_id = parent, "in-process MCP initialization failed")
        }
    }
}
