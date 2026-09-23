use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::ErrorData;
use serde_json::{json, Map, Value};

use super::connection::{INVOKE_TOOL, READ_TOOL, SEARCH_TOOL};
use super::request::{self, RemoteRequest};
use super::{failure, log_failure, RemoteContext, RemoteGateway};

const DEFAULT_SEARCH_LIMIT: usize = 8;

impl RemoteGateway {
    pub(super) async fn search(
        &self,
        mut arguments: Value,
        context: RemoteContext<'_>,
    ) -> Result<Value, ErrorData> {
        let (account, connection) = self.ready_for(context).await?;
        self.ensure_current(&account).await?;
        let group = arguments.get("group_id").and_then(Value::as_str).map(str::to_owned);
        account.browse_arguments(&mut arguments)?;
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
            "search_mode": payload.get("search_mode"),
            "next_cursor": account.project_cursor(&payload, group),
        }))
    }

    pub(in crate::acp::builtin_mcp) async fn read(
        &self,
        capability_id: &str,
        context: RemoteContext<'_>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(name) = capability_id.strip_prefix(super::direct::CAPABILITY_PREFIX) {
            return self.read_direct(name, context).await;
        }
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
        if let Some(name) = call.0.strip_prefix(super::direct::CAPABILITY_PREFIX) {
            return self.invoke_direct(CallToolRequestParams::new(name.to_owned()).with_arguments(call.1), context).await;
        }
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
        let mut query = json!({"limit": arguments.get("limit").filter(|value| !value.is_null()).cloned().unwrap_or(json!(DEFAULT_SEARCH_LIMIT))});
        for key in ["query", "mode", "group_id", "cursor"] {
            if let Some(value) = arguments.get(key).filter(|value| !value.is_null()) { query[key] = value.clone(); }
        }
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
                payload["next_cursor"] = remote["next_cursor"].clone();
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
