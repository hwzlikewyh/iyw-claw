use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::ErrorData;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::connection::{INVOKE_TOOL, READ_TOOL, SEARCH_TOOL};
use super::request::{self, RemoteRequest};
use super::{failure, RemoteAccount, RemoteContext, RemoteGateway};

const DIRECT_PREFIX: &str = "iyw_remote_";
pub(super) const CAPABILITY_PREFIX: &str = "iyw.remote.direct.";
const NAME_HINT_CHARS: usize = 20;
const NAME_DIGEST_CHARS: usize = 24;
const SUMMARY_CHARS: usize = 256;

pub(super) fn is_directory_tool(name: &str) -> bool {
    [SEARCH_TOOL, READ_TOOL, INVOKE_TOOL].contains(&name)
}

impl RemoteAccount {
    fn direct_name(&self, tool: &Tool) -> String {
        let hint: String = tool
            .name
            .chars()
            .take(NAME_HINT_CHARS)
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        let mut hash = Sha256::new();
        hash.update(self.fingerprint);
        hash.update(serde_json::to_vec(tool).unwrap_or_default());
        let digest = format!("{:x}", hash.finalize());
        format!("{DIRECT_PREFIX}{hint}_{}", &digest[..NAME_DIGEST_CHARS])
    }

    pub(super) fn project_direct_tools(&self, tools: &[Tool]) -> Vec<Tool> {
        tools
            .iter()
            .filter(|tool| !is_directory_tool(&tool.name))
            .map(|tool| {
                let mut projected = tool.clone();
                projected.name = self.direct_name(tool).into();
                projected
            })
            .collect()
    }

    pub(super) fn direct_summaries(&self, tools: &[Tool]) -> Vec<Value> {
        self.project_direct_tools(tools)
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name, "capability_id": format!("{CAPABILITY_PREFIX}{}", tool.name),
                    "description": tool.description.as_deref().unwrap_or_default()
                        .chars().take(SUMMARY_CHARS).collect::<String>(),
                })
            })
            .collect()
    }

    fn resolve_direct(&self, name: &str) -> Result<Tool, ErrorData> {
        let overview = self
            .overview
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        overview.tools.iter().find(|tool| !is_directory_tool(&tool.name) && self.direct_name(tool) == name)
            .cloned().ok_or_else(|| failure("remote_tool_changed",
                "Remote top-level tool was removed, changed or belongs to another account. Inspect the latest overview or refresh tools/list before a new call.", false))
    }
}

impl RemoteGateway {
    pub(in crate::acp::builtin_mcp) async fn invoke_direct(
        &self,
        request: CallToolRequestParams,
        context: RemoteContext<'_>,
    ) -> Result<CallToolResult, ErrorData> {
        if request.task.is_some() {
            return Err(failure(
                "remote_task_unsupported",
                "Task-augmented calls are not supported on this gateway",
                false,
            ));
        }
        let (account, connection) = self.ready_for(context).await?;
        let tool = account.resolve_direct(&request.name)?;
        self.ensure_current(&account).await?;
        ensure_request_active(context)?;
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        // 任意远端 JSON Schema 由远端权威校验，不套用内置 schema 子集验证器。
        let result = request::call(
            &connection,
            RemoteRequest {
                name: &tool.name,
                arguments,
                context,
                account_cancel: &account.cancellation,
                execution: true,
            },
        )
        .await;
        if self.ensure_current(&account).await.is_err() {
            return Err(failure(
                "remote_account_changed",
                "Account changed during execution; verify the original operation outcome",
                true,
            ));
        }
        result
    }

    pub(super) async fn read_direct(
        &self,
        name: &str,
        context: RemoteContext<'_>,
    ) -> Result<CallToolResult, ErrorData> {
        let (account, _) = self.current_account().await?;
        let tool = account.resolve_direct(name)?;
        self.ensure_current(&account).await?;
        ensure_request_active(context)?;
        Ok(CallToolResult::structured(
            serde_json::json!({"capability": {
                "capability_id": format!("{CAPABILITY_PREFIX}{name}"), "source": "remote",
                "kind": "tool", "name": name, "description": tool.description,
                "input_schema": tool.input_schema, "outputSchema": tool.output_schema,
                "annotations": tool.annotations, "execution": tool.execution,
                "guidance": "Remote top-level tool definition. Invoke this capability_id if the direct tool is not advertised by the current adapter."
            }}),
        ))
    }
}

fn ensure_request_active(context: RemoteContext<'_>) -> Result<(), ErrorData> {
    if context.request_cancel.is_cancelled() || context.authority_cancel.is_cancelled() {
        return Err(failure(
            "remote_cancelled",
            "Remote request cancelled before dispatch",
            false,
        ));
    }
    Ok(())
}

pub(in crate::acp::builtin_mcp) fn direct_identity<'a>(
    raw: &'a str,
    server: &str,
) -> Option<&'a str> {
    if raw.starts_with(DIRECT_PREFIX) {
        return Some(raw);
    }
    let normalized: String = server
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    for server in [server, &normalized] {
        for prefix in [format!("mcp__{server}__"), format!("{server}__")] {
            if let Some(name) = raw
                .strip_prefix(&prefix)
                .filter(|name| name.starts_with(DIRECT_PREFIX))
            {
                return Some(name);
            }
        }
    }
    None
}
