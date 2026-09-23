use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Map, Value};

use super::connection::{INVOKE_TOOL, READ_TOOL, SEARCH_TOOL};
use super::request::{self, RemoteRequest};
use super::{failure, log_failure, RemoteContext, RemoteGateway};

const DEFAULT_SEARCH_LIMIT: usize = 8;

impl RemoteGateway {
    pub(super) async fn search(
        &self,
        arguments: Value,
        context: RemoteContext<'_>,
    ) -> Result<Value, ErrorData> {
        let (account, connection) = self.ready_for(context).await?;
        self.ensure_current(&account).await?;
        let result = request::call(
            &connection,
            RemoteRequest {
                name: SEARCH_TOOL,
                arguments,
                context,
                account_cancel: &account.cancellation,
                execution: false,
            },
        )
        .await?;
        self.ensure_current(&account).await?;
        let payload = request::payload(result)?;
        let capabilities = account.summaries(&payload)?;
        Ok(json!({
            "capabilities": capabilities,
            "status": if payload.get("degraded") == Some(&Value::Bool(true)) { "degraded" } else { "available" },
            "directory_version": payload.get("directory_version"),
            "match_status": payload.get("match_status"), "reason_code": payload.get("reason_code"),
        }))
    }

    pub(in crate::acp::builtin_mcp) async fn read(
        &self,
        capability_id: &str,
        context: RemoteContext<'_>,
    ) -> Result<CallToolResult, ErrorData> {
        let (account, connection) = self.ready_for(context).await?;
        let route = account.route(capability_id)?;
        self.ensure_current(&account).await?;
        let mut arguments = json!({"id": route.id});
        if connection.tools.iter().any(|tool| {
            tool.name == READ_TOOL
                && tool
                    .input_schema
                    .get("properties")
                    .and_then(|properties| properties.get("include_legacy_tools"))
                    .is_some()
        }) {
            arguments["include_legacy_tools"] = json!(false);
        }
        let result = request::call(
            &connection,
            RemoteRequest {
                name: READ_TOOL,
                arguments,
                context,
                account_cancel: &account.cancellation,
                execution: false,
            },
        )
        .await?;
        self.ensure_current(&account).await?;
        let payload = request::payload(result)?;
        Ok(CallToolResult::structured(json!({
            "capability": account.detail(&payload, &route)?,
            "catalog_digest": payload.get("directory_version"),
            "source": "remote", "instructions": connection.instructions,
            "routing": "Use only returned capability_id with invoke_iyw_capability. The host supplies remote tool_id/tool_version. Preserve usage, argument_sources, prerequisites and business authorization."
        })))
    }

    pub(in crate::acp::builtin_mcp) async fn invoke(
        &self,
        call: (&str, Map<String, Value>),
        context: RemoteContext<'_>,
    ) -> Result<CallToolResult, ErrorData> {
        let (account, connection) = self.ready_for(context).await?;
        let route = account.route(call.0)?;
        if route.group {
            return Err(failure(
                "remote_group_not_invocable",
                "Read the group and invoke a returned member capability_id",
                false,
            ));
        }
        self.ensure_current(&account).await?;
        let result = request::call(&connection, RemoteRequest {
            name: INVOKE_TOOL,
            arguments: json!({"tool_id": route.id, "tool_version": route.version, "arguments": call.1}),
            context, account_cancel: &account.cancellation, execution: true,
        }).await;
        if self.ensure_current(&account).await.is_err() {
            return Err(failure(
                "remote_account_changed",
                "Account changed during execution; original operation outcome may be unknown",
                true,
            ));
        }
        if let Err(error) = &result {
            log_failure("invoke", error);
        }
        result
    }

    pub(in crate::acp::builtin_mcp) async fn merge_search(
        &self,
        search: (CallToolResult, &Value),
        context: RemoteContext<'_>,
    ) -> CallToolResult {
        let (local, arguments) = search;
        let source = arguments
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("all");
        if source == "local" {
            return local;
        }
        let mut payload = local
            .structured_content
            .unwrap_or_else(|| json!({"capabilities": []}));
        if source == "remote" {
            payload["capabilities"] = json!([]);
        }
        let query = json!({"query": arguments.get("query"),
            "limit": arguments.get("limit").cloned().unwrap_or(json!(DEFAULT_SEARCH_LIMIT))});
        match self.search(query, context).await {
            Ok(mut remote) => {
                if let Some(version) = remote["directory_version"].as_str() {
                    payload["catalog_digest"] = json!(format!(
                        "{}:{version}",
                        payload["catalog_digest"].as_str().unwrap_or_default()
                    ));
                }
                if let (Some(values), Some(items)) = (
                    payload["capabilities"].as_array_mut(),
                    remote["capabilities"].as_array(),
                ) {
                    values.extend(items.iter().cloned());
                }
                remote
                    .as_object_mut()
                    .map(|object| object.remove("capabilities"));
                payload["remote_catalog"] = remote;
            }
            Err(error) => {
                log_failure("search", &error);
                payload["remote_catalog"] = json!({"status": "unavailable", "error": error,
                    "guidance": "Local matches remain usable. Remote discovery failed; this does not mean the remote capability or business data is absent."});
            }
        }
        CallToolResult::structured(payload)
    }
}
