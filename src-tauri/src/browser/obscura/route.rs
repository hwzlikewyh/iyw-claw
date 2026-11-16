use std::time::Instant;

use serde_json::{json, Value};

use super::super::agent_timeout::requested_timeout;
use super::super::agent_tool_cancellation::{
    ensure_request_active, AgentOperationCancellation, AgentToolContext,
};
use super::super::agent_tool_support::{default_agent_tab_id, optional_string, COMMAND_TIMEOUT};
use super::super::{BrowserError, BrowserErrorCode, BrowserSessionManager};
use super::actions::{prepare, Action};
use super::worker::Operation;
use super::{Route, Task};

const PROVIDER_BUDGET_DIVISOR: u32 = 2;

pub(super) struct Call<'a> {
    pub(super) context: AgentToolContext<'a>,
    pub(super) action: &'a str,
    pub(super) input: &'a Value,
    pub(super) deadline: Instant,
}

impl BrowserSessionManager {
    pub(in crate::browser) async fn try_obscura(
        &self,
        context: AgentToolContext<'_>,
        action: &str,
        input: &Value,
    ) -> Result<Option<Value>, BrowserError> {
        let supplied = optional_string(input, "tab_id", 128)?;
        if supplied.is_some_and(|id| !id.starts_with("obscura:")) {
            return Ok(None);
        }
        let existing = self.obscura.existing(context.identity).await;
        if existing.is_none() && action != "open" && supplied.is_none() {
            return Ok(None);
        }
        if existing.is_none() && supplied.is_none() {
            let state = self.agent_snapshot_for(context.identity).await;
            if default_agent_tab_id(&state).is_some() {
                return Ok(None);
            }
        }
        let timeout = requested_timeout(input, COMMAND_TIMEOUT)?;
        let deadline = Instant::now() + timeout;
        let task = match existing {
            Some(task) => task,
            None => match self.obscura.task(context.identity).await {
                Some(task) => task,
                None => return Ok(None),
            },
        };
        self.run_obscura_task(
            &task,
            Call {
                context,
                action,
                input,
                deadline,
            },
        )
        .await
    }

    async fn run_obscura_task(
        &self,
        task: &Task,
        call: Call<'_>,
    ) -> Result<Option<Value>, BrowserError> {
        let shutdown = self.shutdown_cancellation().await;
        let request =
            AgentOperationCancellation::new(call.context.cancellation, task.cancellation.clone());
        let cancellation = AgentOperationCancellation::new(&request.token(), shutdown);
        let token = cancellation.token();
        let _cancel_on_return = token.clone().drop_guard();
        let context = AgentToolContext {
            cancellation: &token,
            ..call.context
        };
        let call = Call { context, ..call };
        let mut route = tokio::select! {
            biased;
            _ = token.cancelled() => return Err(super::super::agent_tool_cancellation::cancelled_error()),
            result = tokio::time::timeout_at(call.deadline.into(), task.route.lock()) => result.map_err(|_| super::worker::timeout())?,
        };
        ensure_request_active(context)?;
        self.ensure_managed_browser_enabled()?;
        if route.managed {
            if let Some(id) = call.input["tab_id"]
                .as_str()
                .filter(|id| id.starts_with("obscura:"))
            {
                return Err(BrowserError::tab_not_found(id));
            }
            return Ok(None);
        }
        if call.action == "list_tabs" {
            return self.list_with_obscura(&route, context).await.map(Some);
        }
        if call.action == "close_window" {
            return Ok(Some(route.project(
                None,
                Some(json!({"closed":false,"reason":"not_presented"})),
            )));
        }
        self.execute_obscura(&mut route, &call).await.map(Some)
    }

    async fn execute_obscura(
        &self,
        route: &mut Route,
        call: &Call<'_>,
    ) -> Result<Value, BrowserError> {
        let prepared = prepare(call.action, call.input)?;
        let supplied = optional_string(call.input, "tab_id", 128)?;
        if supplied.is_some_and(|id| !route.tabs.contains_key(id)) {
            return Err(BrowserError::tab_not_found(supplied.unwrap_or_default()));
        }
        if matches!(prepared, Action::Managed) {
            if route.tabs.values().any(|tab| tab.interacted)
                && !matches!(call.action, "present" | "request_user_action")
            {
                return Err(BrowserError::new(BrowserErrorCode::BrowserRuntimeUnavailable,
                    "This operation requires the built-in browser; the current Obscura page has interaction state and cannot be automatically replayed"));
            }
            return self.fallback_from_obscura(route, call, "capability").await;
        }
        let remaining = call.deadline.saturating_duration_since(Instant::now());
        let budget = if route.tabs.values().any(|tab| tab.interacted) {
            remaining
        } else {
            remaining / PROVIDER_BUDGET_DIVISOR
        };
        let operation = Operation {
            cancellation: call.context.cancellation,
            deadline: Instant::now() + budget,
        };
        let result = route.run(prepared, call.input, &operation).await;
        match result {
            Ok(value) => Ok(value),
            Err(error)
                if can_fallback(route, call.action, &error)
                    && !call.context.cancellation.is_cancelled() =>
            {
                tracing::info!(connection_id = %call.context.identity.connection_id, error_code = ?error.code,
                    "Embedded Obscura is unavailable; falling back to the built-in browser");
                self.fallback_from_obscura(route, call, "provider_failure")
                    .await
            }
            Err(error) => Err(error),
        }
    }

    async fn list_with_obscura(
        &self,
        route: &Route,
        context: AgentToolContext<'_>,
    ) -> Result<Value, BrowserError> {
        let mut result = route.project(None, None);
        let managed = self.agent_state(context, None, None).await?;
        if let (Some(tabs), Some(existing)) =
            (result["tabs"].as_array_mut(), managed["tabs"].as_array())
        {
            tabs.extend(existing.iter().cloned());
        }
        if managed["activeTabId"].is_string() {
            result["activeTabId"] = managed["activeTabId"].clone();
        }
        result["dialogs"] = managed["dialogs"].clone();
        result["downloads"] = managed["downloads"].clone();
        Ok(result)
    }
}

pub(super) fn selected_id<'a>(route: &'a Route, input: &'a Value) -> Option<&'a str> {
    input["tab_id"].as_str().or(route.active.as_deref())
}

fn can_fallback(route: &Route, action: &str, error: &BrowserError) -> bool {
    matches!(action, "open" | "read" | "snapshot" | "wait" | "screenshot")
        && !error.effect_may_have_occurred
        && !route.tabs.values().any(|tab| tab.interacted)
        && matches!(
            error.code,
            BrowserErrorCode::BrowserRuntimeUnavailable
                | BrowserErrorCode::BrowserNavigationFailed
                | BrowserErrorCode::BrowserOperationTimeout
                | BrowserErrorCode::BrowserFrameDecodeFailed
        )
}
