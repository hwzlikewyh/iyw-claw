use serde_json::Value;

use super::agent_tool_cancellation::{
    cancelled_error, ensure_request_active, AgentOperationCancellation, AgentToolContext,
};
use super::agent_tool_support::{
    agent_access, browser_error, invalid_argument, optional_string, project_agent_state,
    required_string,
};
use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::types::{BrowserAgentIdentity, BrowserAgentToolCall, BrowserHostKind};

impl BrowserSessionManager {
    pub async fn execute_agent_tool(&self, call: BrowserAgentToolCall) -> Value {
        let tab_id = call.input.get("tab_id").and_then(Value::as_str);
        let context = AgentToolContext {
            identity: &call.identity,
            cancellation: &call.cancellation,
        };
        tracing::info!(
            target: "iyw_claw_browser",
            connection_id = %call.identity.connection_id,
            conversation_id = call.identity.conversation_id,
            browser_tab_id = tab_id,
            tool = %call.tool,
            "Agent browser tool started"
        );
        let result = self
            .dispatch_agent_tool(context, &call.tool, &call.input)
            .await;
        match result {
            Ok(value) => {
                tracing::info!(
                    target: "iyw_claw_browser",
                    connection_id = %call.identity.connection_id,
                    browser_tab_id = tab_id,
                    tool = %call.tool,
                    "Agent browser tool completed"
                );
                value
            }
            Err(error) => {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    connection_id = %call.identity.connection_id,
                    browser_tab_id = tab_id,
                    tool = %call.tool,
                    error_code = ?error.code,
                    retryable = error.retryable,
                    "Agent browser tool failed"
                );
                browser_error(error)
            }
        }
    }

    async fn dispatch_agent_tool(
        &self,
        context: AgentToolContext<'_>,
        tool: &str,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        match tool {
            "browser_list_tabs" => {
                ensure_request_active(context)?;
                self.agent_state(context.identity, None, None).await
            }
            "browser_open" => self.agent_open(context, input).await,
            "browser_snapshot" => self.agent_snapshot(context, input).await,
            "browser_click" => self.agent_click(context, input).await,
            "browser_fill" => self.agent_fill(context, input).await,
            "browser_press" => self.agent_press(context, input).await,
            "browser_scroll" => self.agent_scroll(context, input).await,
            "browser_wait" => self.agent_wait(context, input).await,
            "browser_screenshot" => self.agent_screenshot(context, input).await,
            "browser_close_tab" => self.agent_close(context, input).await,
            _ => Err(invalid_argument("Unknown browser tool")),
        }
    }

    async fn agent_open(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        let url = required_string(input, "url", 8_192)?;
        if let Some(tab_id) = optional_string(input, "tab_id", 128)? {
            self.agent_navigate(context, tab_id, url).await?;
            return self.agent_state(context.identity, Some(tab_id), None).await;
        }
        let visible = self.snapshot_for_agent(context.identity).await;
        if visible.tabs.is_empty() {
            return Err(BrowserError::new(
                super::error::BrowserErrorCode::BrowserTabAccessDenied,
                "Ask the user to click Share tab with Agent, then call browser_list_tabs and pass tabs[].browserTabId as tab_id",
            ));
        }
        let host_id = self.preferred_agent_host().await;
        let (_, created) = self
            .create_browser_tab_with_id(url.to_string(), agent_access(context.identity), host_id)
            .await?;
        let state = self.snapshot_for_agent(context.identity).await;
        if context.cancellation.is_cancelled() {
            let cleanup_failed = self.close_browser_tab(&created).await.is_err();
            return Err(cancelled_error().effect_may_have_occurred(cleanup_failed));
        }
        Ok(project_agent_state(state, Some(&created), None))
    }

    async fn agent_navigate(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        url: &str,
    ) -> Result<(), BrowserError> {
        let lease = self.acquire_agent_lease(context, tab_id).await?;
        let changed = lease.cancellation_error();
        let cancellation =
            AgentOperationCancellation::new(context.cancellation, lease.cancellation_token());
        let result = self
            .navigate_browser_tab_as_agent(tab_id, url.to_string(), cancellation.token())
            .await;
        lease.finish().await;
        if cancellation.token().is_cancelled() {
            return Err(changed.effect_may_have_occurred(true));
        }
        result.map(|_| ())
    }

    async fn agent_close(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let lease = self.acquire_agent_lease(context, tab_id).await?;
        ensure_request_active(context)?;
        let result = self.close_browser_tab(tab_id).await;
        lease.finish().await;
        result?;
        self.agent_state(context.identity, None, None).await
    }

    pub(super) async fn agent_state(
        &self,
        identity: &BrowserAgentIdentity,
        active_tab_id: Option<&str>,
        output: Option<Value>,
    ) -> Result<Value, BrowserError> {
        Ok(project_agent_state(
            self.snapshot_for_agent(identity).await,
            active_tab_id,
            output,
        ))
    }

    async fn preferred_agent_host(&self) -> Option<String> {
        self.snapshot()
            .await
            .hosts
            .into_iter()
            .find(|host| host.kind == BrowserHostKind::Docked && host.visible)
            .map(|host| host.host_id)
    }
}
