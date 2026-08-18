use serde_json::Value;

use super::agent_tool_actions::AgentCliRequest;
use super::agent_tool_cancellation::AgentToolContext;
use super::agent_tool_support::{required_string, COMMAND_TIMEOUT, SNAPSHOT_TIMEOUT};
use super::error::BrowserError;
use super::manager::BrowserSessionManager;

impl BrowserSessionManager {
    pub(super) async fn agent_snapshot(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let mut args = vec!["snapshot".to_string()];
        if input.get("interactive").and_then(Value::as_bool) != Some(false) {
            args.push("--interactive".to_string());
        }
        if input.get("compact").and_then(Value::as_bool) != Some(false) {
            args.push("--compact".to_string());
        }
        let output = self
            .run_agent_cli(AgentCliRequest {
                context,
                tab_id,
                args,
                timeout: SNAPSHOT_TIMEOUT,
            })
            .await?;
        self.agent_state(context, Some(tab_id), Some(output)).await
    }

    pub(super) async fn agent_screenshot(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let mut args = vec!["screenshot".to_string()];
        if input.get("full_page").and_then(Value::as_bool) == Some(true) {
            args.push("--full".to_string());
        }
        if input.get("annotate").and_then(Value::as_bool) == Some(true) {
            args.push("--annotate".to_string());
        }
        let output = self
            .run_agent_cli(AgentCliRequest {
                context,
                tab_id,
                args,
                timeout: COMMAND_TIMEOUT,
            })
            .await?;
        self.enforce_agent_screenshot_quota().await?;
        self.agent_state(context, Some(tab_id), Some(output)).await
    }

    async fn enforce_agent_screenshot_quota(&self) -> Result<(), BrowserError> {
        let runtime = self.desktop_runtime()?;
        let context = runtime.context().await.ok_or_else(|| {
            BrowserError::new(
                super::error::BrowserErrorCode::BrowserRuntimeUnavailable,
                "The browser runtime is unavailable",
            )
        })?;
        super::screenshot_quota::enforce_screenshot_quota(context.cli.screenshot_path()).await
    }
}
