use std::time::Instant;

use serde_json::{json, Value};

use super::super::agent_tool_cancellation::{ensure_request_active, AgentToolContext};
use super::super::agent_tool_support::browser_error;
use super::super::{BrowserError, BrowserErrorCode, BrowserErrorContext, BrowserSessionManager};
use super::route::{selected_id, Call};
use super::Route;

impl BrowserSessionManager {
    pub(super) async fn fallback_from_obscura(
        &self,
        route: &mut Route,
        call: &Call<'_>,
        reason: &str,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(call.context)?;
        self.ensure_managed_browser_enabled()?;
        let url = fallback_url(route, call)?;
        let invalidated = route.tabs.keys().cloned().collect::<Vec<_>>();
        route.worker = None;
        route.managed = true;
        route.tabs.clear();
        route.active = None;
        let opening = json!({"url":url,"new_tab":true});
        let opened = self
            .run_managed_with_deadline(call, "open", opening)
            .await?;
        let id = opened["targetTabId"]
            .as_str()
            .or_else(|| opened["activeTabId"].as_str())
            .ok_or_else(|| super::worker::unavailable("The built-in browser did not return a tab"))?
            .to_string();
        let result = if call.action == "open" {
            Ok(opened)
        } else if call.action == "advanced" {
            self.fallback_snapshot(call, &id).await
        } else {
            let mut input = call.input.clone();
            input["tab_id"] = json!(id);
            self.run_managed_with_deadline(call, call.action, input)
                .await
        };
        let mut result = result.unwrap_or_else(|mut error| {
            let mut context = error.context.take().unwrap_or_default();
            context.browser_tab_id = Some(id.clone());
            browser_error(error.with_context(context))
        });
        result["provider"] = json!("managed");
        result["targetTabId"] = json!(id);
        result["fallback"] = json!({"from":"obscura","reason":reason,"stateReset":true,"invalidatedTabIds":invalidated});
        Ok(result)
    }

    async fn run_managed_with_deadline(
        &self,
        call: &Call<'_>,
        action: &str,
        mut input: Value,
    ) -> Result<Value, BrowserError> {
        let remaining = call.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(super::worker::timeout());
        }
        input["timeout_ms"] = json!(remaining.as_millis().max(1) as u64);
        let token = call.context.cancellation.child_token();
        let _cancel_on_return = token.clone().drop_guard();
        let context = AgentToolContext {
            cancellation: &token,
            ..call.context
        };
        tokio::time::timeout(remaining, self.run_managed_action(context, action, &input))
            .await
            .map_err(|_| super::worker::timeout().effect_may_have_occurred(true))?
    }

    async fn fallback_snapshot(&self, call: &Call<'_>, id: &str) -> Result<Value, BrowserError> {
        let snapshot = self
            .run_managed_with_deadline(call, "snapshot", json!({"tab_id":id}))
            .await?;
        let mut result = browser_error(BrowserError::new(BrowserErrorCode::BrowserSnapshotStale,
            "The page moved to the built-in browser. The advanced command was not executed; use the fresh snapshot and tab ID before continuing")
            .with_context(BrowserErrorContext { browser_tab_id:Some(id.into()), ..BrowserErrorContext::default() }));
        result["recovery"] = snapshot;
        Ok(result)
    }
}

fn fallback_url(route: &Route, call: &Call<'_>) -> Result<String, BrowserError> {
    let selected = selected_id(route, call.input).and_then(|id| route.tabs.get(id));
    let url = if call.action == "open" {
        call.input["url"].as_str()
    } else {
        selected.map(|tab| tab.url.as_str())
    };
    super::super::tab_actions::validated_url(url.ok_or_else(|| {
        BrowserError::new(
            BrowserErrorCode::BrowserTabNotFound,
            "Open a browser page before requesting this operation",
        )
    })?)
}
