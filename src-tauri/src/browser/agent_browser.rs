use serde_json::Value;

use super::agent_browser_input::{browser_action, managed_input, managed_semantic_command};
use super::agent_tool_cancellation::{ensure_request_active, AgentToolContext};
use super::agent_tool_support::invalid_argument;
use super::error::BrowserError;
use super::manager::BrowserSessionManager;

impl BrowserSessionManager {
    pub(super) async fn agent_browser(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        self.ensure_managed_browser_enabled()?;
        let action = browser_action(input)?;
        let input = managed_input(&action, input)?;
        let mut result = self.run_managed_action(context, &action, &input).await?;
        if let Some(object) = result.as_object_mut() {
            object.insert("provider".into(), Value::String("managed".into()));
        }
        Ok(result)
    }

    async fn run_managed_action(
        &self,
        context: AgentToolContext<'_>,
        action: &str,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        match action {
            "list_tabs" => self.agent_state(context, None, None).await,
            "open" => self.agent_open(context, input).await,
            "snapshot" => self.agent_snapshot(context, input).await,
            "read" => self.agent_read(context, input).await,
            "screenshot" => self.agent_screenshot(context, input).await,
            "close_tab" => self.agent_close(context, input).await,
            "present" => self.agent_present_window(context, input).await,
            "close_window" => self.agent_close_window(context, input).await,
            _ => self.run_managed_interaction(context, action, input).await,
        }
    }

    async fn run_managed_interaction(
        &self,
        context: AgentToolContext<'_>,
        action: &str,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        if let Some(command) = managed_semantic_command(action, input)? {
            return self.agent_command(context, &command).await;
        }
        match action {
            "click" => self.agent_click(context, input).await,
            "fill" => self.agent_fill(context, input).await,
            "press" => self.agent_press(context, input).await,
            "scroll" => self.agent_scroll(context, input).await,
            "wait" => self.agent_wait(context, input).await,
            "advanced" => self.agent_command(context, input).await,
            "request_user_action" => self.agent_request_user_action(context, input).await,
            _ => Err(invalid_argument("Unsupported browser action")),
        }
    }
}
