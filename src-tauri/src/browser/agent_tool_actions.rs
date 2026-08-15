use std::time::Duration;

use serde_json::Value;

use super::agent_tool_cancellation::{AgentOperationCancellation, AgentToolContext};
use super::agent_tool_support::{
    invalid_argument, optional_string, required_string, COMMAND_TIMEOUT, MAX_KEY_CHARS,
    MAX_SELECTOR_CHARS, MAX_TEXT_CHARS, MAX_WAIT_MS,
};
use super::error::BrowserError;
use super::manager::BrowserSessionManager;

pub(super) struct AgentCliRequest<'a> {
    pub context: AgentToolContext<'a>,
    pub tab_id: &'a str,
    pub args: Vec<String>,
    pub timeout: Duration,
}

impl BrowserSessionManager {
    pub(super) async fn agent_click(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let selector = required_string(input, "selector", MAX_SELECTOR_CHARS)?;
        self.run_and_project(context, tab_id, vec!["click", selector])
            .await
    }

    pub(super) async fn agent_fill(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let selector = required_string(input, "selector", MAX_SELECTOR_CHARS)?;
        let text = optional_string(input, "text", MAX_TEXT_CHARS)?
            .ok_or_else(|| invalid_argument("Missing browser argument: text"))?;
        self.run_and_project(context, tab_id, vec!["fill", selector, text])
            .await
    }

    pub(super) async fn agent_press(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let key = required_string(input, "key", MAX_KEY_CHARS)?;
        self.run_and_project(context, tab_id, vec!["press", key])
            .await
    }

    pub(super) async fn agent_scroll(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let direction = required_string(input, "direction", 8)?;
        if !matches!(direction, "up" | "down" | "left" | "right") {
            return Err(invalid_argument("Invalid scroll direction"));
        }
        let pixels = input
            .get("pixels")
            .and_then(Value::as_u64)
            .unwrap_or(600)
            .clamp(1, 10_000)
            .to_string();
        self.run_and_project(context, tab_id, vec!["scroll", direction, &pixels])
            .await
    }

    pub(super) async fn agent_wait(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let target = optional_string(input, "selector", MAX_SELECTOR_CHARS)?
            .map(str::to_string)
            .unwrap_or_else(|| wait_milliseconds(input));
        self.run_and_project(context, tab_id, vec!["wait", &target])
            .await
    }

    async fn run_and_project(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        args: Vec<&str>,
    ) -> Result<Value, BrowserError> {
        let output = self
            .run_agent_cli(AgentCliRequest {
                context,
                tab_id,
                args: args.into_iter().map(str::to_string).collect(),
                timeout: COMMAND_TIMEOUT,
            })
            .await?;
        self.agent_state(context.identity, Some(tab_id), Some(output))
            .await
    }

    pub(super) async fn run_agent_cli(
        &self,
        request: AgentCliRequest<'_>,
    ) -> Result<Value, BrowserError> {
        let lease = self
            .acquire_agent_lease(request.context, request.tab_id)
            .await?;
        let action = self.tabs.action_target(request.tab_id).await?;
        let changed = lease.cancellation_error();
        let cancellation = AgentOperationCancellation::new(
            request.context.cancellation,
            lease.cancellation_token(),
        );
        let refs = request.args.iter().map(String::as_str).collect::<Vec<_>>();
        let result = action
            .cli
            .run_pinned(
                &action.session,
                &action.cdp_url,
                &refs,
                request.timeout,
                cancellation.token(),
            )
            .await;
        lease.finish().await;
        if cancellation.token().is_cancelled() {
            return Err(changed.effect_may_have_occurred(true));
        }
        result
    }
}

fn wait_milliseconds(input: &Value) -> String {
    input
        .get("milliseconds")
        .and_then(Value::as_u64)
        .unwrap_or(1_000)
        .clamp(1, MAX_WAIT_MS)
        .to_string()
}
