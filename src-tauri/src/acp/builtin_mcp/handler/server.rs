use std::future::Future;

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

impl ServerHandler for BuiltinMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("iyw-claw", env!("CARGO_PKG_VERSION")))
            .with_instructions(crate::acp::builtin_mcp::service::SERVER_INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        async move {
            let (authority, _) = Self::authenticated(&context.extensions)?;
            ensure_active(&authority, &context.ct)?;
            let tools = gateway::tools()
                .map_err(catalog_error)?
                .into_iter()
                .filter(|tool| match tool.name.as_ref() {
                    "ask_user_question" | "show_interactive_html" => {
                        authority.features().should_list("ask_user_question")
                    }
                    "present_task_files" => authority.features().should_list("present_task_files"),
                    _ => true,
                })
                .collect::<Vec<_>>();
            log_tools_list(&authority, tools.len());
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
