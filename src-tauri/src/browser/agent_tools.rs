use serde_json::Value;

use super::agent_tool_cancellation::{
    cancelled_error, ensure_request_active, AgentOperationCancellation, AgentToolContext,
};
use super::agent_tool_support::{
    browser_error, default_agent_tab_id, invalid_argument, optional_bool, optional_string,
    preferred_agent_host_id, project_agent_state, required_string,
};
use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::types::{BrowserAgentToolCall, BrowserStateSnapshot};

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
            turn_generation = call.identity.turn_generation,
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
                self.agent_state(context, None, None).await
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
            "browser_request_user_action" => self.agent_request_user_action(context, input).await,
            "browser_present" => self.agent_present_window(context, input).await,
            "browser_close_window" => self.agent_close_window(context, input).await,
            _ => Err(invalid_argument("Unknown browser tool")),
        }
    }

    pub(super) async fn agent_open(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        let url = required_string(input, "url", 8_192)?;
        let tab_id = optional_string(input, "tab_id", 128)?;
        let new_tab = optional_bool(input, "new_tab")?.unwrap_or(false);
        if tab_id.is_some() && new_tab {
            return Err(invalid_argument(
                "browser_open cannot combine tab_id with new_tab=true",
            ));
        }
        if let Some(tab_id) = tab_id {
            tracing::info!(
                target: "iyw_claw_browser",
                browser_tab_id = tab_id,
                open_mode = "explicit_tab",
                "Agent browser open selected tab"
            );
            self.agent_navigate(context, tab_id, url).await?;
            return self.agent_state(context, Some(tab_id), None).await;
        }
        let epoch = self.current_shutdown_epoch();
        let _guard = tokio::select! {
            _ = context.cancellation.cancelled() => return Err(cancelled_error()),
            guard = self.tab_open_lock.lock() => guard,
        };
        ensure_request_active(context)?;
        self.ensure_shutdown_epoch(epoch)?;
        let state = self.agent_snapshot_for(context.identity).await;
        if !new_tab {
            if let Some(target) = default_agent_tab_id(&state) {
                tracing::info!(
                    target: "iyw_claw_browser",
                    browser_tab_id = %target,
                    open_mode = "active_tab",
                    "Agent browser open selected tab"
                );
                drop(_guard);
                self.agent_navigate(context, &target, url).await?;
                return self.agent_state(context, Some(&target), None).await;
            }
        }
        tracing::info!(
            target: "iyw_claw_browser",
            open_mode = if new_tab { "explicit_new_tab" } else { "initial_tab" },
            "Agent browser open is creating a tab"
        );
        let created = self
            .agent_create_locked(context.identity, url, &state)
            .await;
        drop(_guard);
        let created = created?;
        if context.cancellation.is_cancelled() {
            let cleanup_failed = self.close_browser_tab(&created).await.is_err();
            return Err(cancelled_error().effect_may_have_occurred(cleanup_failed));
        }
        self.agent_state(context, Some(&created), None).await
    }

    async fn agent_create_locked(
        &self,
        identity: &super::types::BrowserAgentIdentity,
        url: &str,
        state: &BrowserStateSnapshot,
    ) -> Result<String, BrowserError> {
        let host_id = preferred_agent_host_id(state);
        let headless = host_id.is_none();
        let cancellation = self.shutdown_cancellation().await;
        let (_, created) = self
            .create_browser_tab_with_id_unlocked(url.to_string(), host_id, cancellation)
            .await?;
        self.agent_turn_leases.register(identity, &created).await?;
        if headless {
            self.agent_turn_leases
                .mark_close_pending(std::slice::from_ref(&created))
                .await;
        }
        tracing::info!(
            target: "iyw_claw_browser",
            browser_tab_id = %created,
            "Agent browser tab created"
        );
        Ok(created)
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
        self.agent_state(context, Some(tab_id), None).await
    }

    pub(super) async fn agent_state(
        &self,
        context: AgentToolContext<'_>,
        target_tab_id: Option<&str>,
        output: Option<Value>,
    ) -> Result<Value, BrowserError> {
        Ok(project_agent_state(
            self.agent_snapshot_for(context.identity).await,
            target_tab_id,
            output,
        ))
    }
}
