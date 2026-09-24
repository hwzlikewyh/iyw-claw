use std::future::Future;
use std::sync::Mutex;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};

use super::BuiltinMcpHandler;
use crate::acp::builtin_mcp::diagnostics::log_tools_list;
use crate::acp::builtin_mcp::gateway;
use crate::acp::builtin_mcp::invocation::ensure_active;
use crate::acp::builtin_mcp::result::catalog_error;

const NOTIFICATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

impl ServerHandler for BuiltinMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::new("iyw-claw", env!("CARGO_PKG_VERSION")))
        .with_instructions(format!(
            "{} {}",
            crate::acp::builtin_mcp::service::SERVER_INSTRUCTIONS,
            super::super::remote_mcp::AGENT_INSTRUCTIONS
        ))
    }

    fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        async move {
            let (authority, delivery) = Self::authenticated(&context.extensions)?;
            ensure_active(&authority, &context.ct)?;
            self.remote_updates
                .subscribe(&self.iyw.remote, &authority, context.peer.clone());
            let (overview, direct_tools) = self.iyw.remote.advertised_catalog().await;
            ensure_active(&authority, &context.ct)?;
            let mut tools = gateway::tools()
                .map_err(catalog_error)?
                .into_iter()
                .map(|tool| attach_remote_overview(tool, &overview))
                .filter_map(|tool| {
                    if tool.name == "manage_iyw_memory" {
                        crate::acp::builtin_mcp::iyw_memory::project_tool(
                            tool,
                            authority.features(),
                        )
                    } else {
                        Some(tool)
                    }
                })
                .filter(|tool| match tool.name.as_ref() {
                    "ask_user_question" | "show_interactive_html" => {
                        authority.features().should_list("ask_user_question")
                    }
                    "present_task_files" => authority.features().should_list("present_task_files"),
                    _ => true,
                })
                .collect::<Vec<_>>();
            tools.extend(direct_tools);
            ensure_active(&authority, &context.ct)?;
            let names = tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>();
            log_tools_list(&authority, &names);
            if let Some(delivery) = delivery {
                let ready = authority.tools_ready();
                let generation = authority.tools_generation();
                delivery.register(
                    Box::new(move || {
                        if !authority.cancellation().is_cancelled() {
                            ready.delivered(generation);
                        }
                    }),
                    Box::new(|| {}),
                );
            }
            Ok(ListToolsResult::with_all_items(tools))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + MaybeSendFuture + '_ {
        self.call(request, context)
    }
}

fn attach_remote_overview(mut tool: rmcp::model::Tool, overview: &str) -> rmcp::model::Tool {
    if tool.name == "search_iyw_capabilities" {
        tool.description = Some(
            format!(
                "{}\n\n{overview}",
                tool.description.as_deref().unwrap_or_default()
            )
            .into(),
        );
    }
    tool
}

#[derive(Default)]
pub(super) struct RemoteUpdates {
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl RemoteUpdates {
    fn subscribe(
        &self,
        remote: &std::sync::Arc<crate::acp::builtin_mcp::remote_mcp::RemoteGateway>,
        authority: &crate::acp::builtin_mcp::authority::SessionContext,
        peer: rmcp::service::Peer<RoleServer>,
    ) {
        let mut slot = self.task.lock().unwrap_or_else(|error| error.into_inner());
        if slot.as_ref().is_some_and(|task| !task.is_finished()) {
            return;
        }
        let mut changes = remote.subscribe_catalog();
        let cancel = authority.cancellation().clone();
        *slot = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    changed = changes.changed() => { if changed.is_err() { return; } },
                }
                if peer.is_transport_closed() {
                    return;
                }
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    result = tokio::time::timeout(NOTIFICATION_TIMEOUT, peer.notify_tool_list_changed()) => {
                        if !matches!(result, Ok(Ok(()))) { return; }
                    }
                }
            }
        }));
    }
}

impl Drop for RemoteUpdates {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
    }
}
