use std::time::Duration;

use serde_json::Value;

use super::agent_timeout::requested_timeout;

use super::agent_tool_cancellation::{AgentOperationCancellation, AgentToolContext};
use super::agent_tool_support::{
    invalid_argument, optional_string, required_string, COMMAND_TIMEOUT, MAX_KEY_CHARS,
    MAX_SELECTOR_CHARS, MAX_TEXT_CHARS, MAX_WAIT_MS,
};
use super::error::{BrowserError, BrowserErrorContext};
use super::manager::BrowserSessionManager;

const WAIT_COMPLETION_MARGIN: Duration = Duration::from_secs(5);

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
        let selector = required_string(input, "selector", MAX_SELECTOR_CHARS)?;
        self.run_and_project(context, input, vec!["click", selector])
            .await
    }

    pub(super) async fn agent_fill(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let selector = required_string(input, "selector", MAX_SELECTOR_CHARS)?;
        let text = optional_string(input, "text", MAX_TEXT_CHARS)?
            .ok_or_else(|| invalid_argument("Missing browser argument: text"))?;
        self.run_and_project(context, input, vec!["fill", selector, text])
            .await
    }

    pub(super) async fn agent_press(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let key = required_string(input, "key", MAX_KEY_CHARS)?;
        self.run_and_project(context, input, vec!["press", key])
            .await
    }

    pub(super) async fn agent_scroll(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
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
        self.run_and_project(context, input, vec!["scroll", direction, &pixels])
            .await
    }

    pub(super) async fn agent_wait(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let selector = optional_string(input, "selector", MAX_SELECTOR_CHARS)?;
        let milliseconds = wait_milliseconds(input);
        let timeout = if selector.is_some() {
            COMMAND_TIMEOUT
        } else {
            COMMAND_TIMEOUT.max(Duration::from_millis(milliseconds) + WAIT_COMPLETION_MARGIN)
        };
        let timeout = requested_timeout(input, timeout)?;
        let target = selector
            .map(str::to_string)
            .unwrap_or_else(|| milliseconds.to_string());
        let output = self
            .run_agent_cli(AgentCliRequest {
                context,
                tab_id,
                args: vec!["wait".to_string(), target],
                timeout,
            })
            .await?;
        self.agent_state(context, Some(tab_id), Some(output)).await
    }

    async fn run_and_project(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
        args: Vec<&str>,
    ) -> Result<Value, BrowserError> {
        let tab_id = required_string(input, "tab_id", 128)?;
        let output = self
            .run_agent_cli(AgentCliRequest {
                context,
                tab_id,
                args: args.into_iter().map(str::to_string).collect(),
                timeout: requested_timeout(input, COMMAND_TIMEOUT)?,
            })
            .await?;
        self.agent_state(context, Some(tab_id), Some(output)).await
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
        result.map_err(|error| {
            error.with_context(BrowserErrorContext {
                browser_tab_id: Some(request.tab_id.to_string()),
                runtime_generation: Some(action.runtime_generation),
                ..BrowserErrorContext::default()
            })
        })
    }
}

fn wait_milliseconds(input: &Value) -> u64 {
    input
        .get("milliseconds")
        .and_then(Value::as_u64)
        .unwrap_or(1_000)
        .clamp(1, MAX_WAIT_MS)
}
