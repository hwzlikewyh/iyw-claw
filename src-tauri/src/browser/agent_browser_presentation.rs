use serde_json::{json, Value};

use super::agent_browser::{BrowserRoute, BrowserRouteProvider};
use super::agent_browser_handoff_state::current_url;
use super::agent_tool_cancellation::AgentToolContext;
use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;

impl BrowserSessionManager {
    pub(super) async fn present_opencli_page(
        &self,
        context: AgentToolContext<'_>,
        key: &str,
        session: &str,
        target: Option<&str>,
        display_tab: Option<&str>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let url = current_url(session, target, input).await?;
        let state = self.agent_snapshot_for(context.identity).await;
        let existing =
            display_tab.filter(|id| state.tabs.iter().any(|tab| tab.browser_tab_id == *id));
        let open_input = match existing {
            Some(tab_id) => json!({ "url": url, "tab_id": tab_id }),
            None => json!({ "url": url, "new_tab": true }),
        };
        let opened = self
            .run_managed_action(context, "open", &open_input, Some("opencli_present"))
            .await?;
        let tab_id = opened
            .get("targetTabId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                BrowserError::new(
                    BrowserErrorCode::BrowserInternal,
                    "Managed browser did not return a tab id",
                )
            })?;
        let presented = self
            .run_managed_action(
                context,
                "present",
                &json!({ "tab_id": tab_id }),
                Some("opencli_present"),
            )
            .await?;
        let request_id = presented
            .get("output")
            .and_then(|value| value.get("requestId"))
            .cloned();
        self.store_browser_route(
            key,
            BrowserRoute {
                provider: BrowserRouteProvider::Opencli {
                    session: session.to_string(),
                    target: target.map(str::to_string),
                    display_tab: Some(tab_id.to_string()),
                },
            },
        )
        .await;
        tracing::info!(
            target: "iyw_claw_browser",
            operation_provider = "opencli",
            display_provider = "managed",
            display_tab_id = %tab_id,
            request_id = ?request_id,
            "OpenCLI page copied to managed browser for presentation"
        );
        Ok(json!({
            "ok": true,
            "provider": "opencli",
            "browserTabId": opencli_browser_tab_id(session, target),
            "presentation": {
                "provider": "managed",
                "browserTabId": tab_id,
                "status": "present_requested",
                "requestId": request_id,
            },
            "output": {
                "status": "present_requested",
                "preservesProvider": "opencli",
                "displayTabId": tab_id,
            },
        }))
    }
}

pub(super) fn opencli_browser_tab_id(session: &str, target: Option<&str>) -> String {
    target
        .map(|target| format!("opencli:{session}:{target}"))
        .unwrap_or_else(|| format!("opencli:{session}"))
}
