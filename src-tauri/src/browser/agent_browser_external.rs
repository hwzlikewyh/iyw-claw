use serde_json::{json, Value};

use super::agent_browser::{BrowserRoute, BrowserRouteProvider};
use super::agent_browser_input::browser_action;
use super::agent_browser_presentation::opencli_browser_tab_id;
use super::agent_browser_request_support::requested_target;
use super::agent_browser_route::{input_requests_managed, route_key, session_name};
use super::agent_tool_support::invalid_argument;
use super::opencli::{OpencliFailure, OpencliProvider};
use super::{BrowserAgentIdentity, BrowserError, BrowserSessionManager};

const EXTERNAL_STATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

impl BrowserSessionManager {
    pub(super) async fn browser_route_for_input(
        &self,
        identity: &BrowserAgentIdentity,
        input: &Value,
    ) -> Result<Option<BrowserRoute>, BrowserError> {
        let key = route_key(identity, input);
        let mut routes = self.browser_routes.lock().await;
        let route = routes.get(&key).cloned();
        if self.managed_browser_enabled() {
            return Ok(route);
        }
        if input_requests_managed(input) {
            self.ensure_managed_browser_enabled()?;
        }
        if matches!(
            route.as_ref().map(|r| &r.provider),
            Some(BrowserRouteProvider::Managed { .. })
        ) {
            if browser_action(input)? != "open" {
                self.ensure_managed_browser_enabled()?;
            }
            routes.remove(&key);
            tracing::info!(target: "iyw_claw_browser",
                "Discarded managed browser route because the built-in browser is disabled");
        }
        Ok(route.filter(|r| matches!(r.provider, BrowserRouteProvider::Opencli { .. })))
    }

    pub(super) async fn start_opencli_route(
        &self,
        key: &str,
        identity: &BrowserAgentIdentity,
    ) -> Result<BrowserRoute, BrowserError> {
        let provider = match OpencliProvider::doctor().await {
            Ok(_) => BrowserRouteProvider::Opencli {
                session: session_name(identity),
                target: None,
                display_tab: None,
            },
            Err(failure)
                if matches!(failure.code.as_str(), "OPENCLI_NOT_INSTALLED" | "OPENCLI_BRIDGE_UNAVAILABLE") && self.managed_browser_enabled() =>
            {
                tracing::info!(target: "iyw_claw_browser",
                    reason = %failure.code, "OpenCLI preflight unavailable; using the enabled built-in browser");
                BrowserRouteProvider::Managed {
                    reason: Some(failure.code),
                }
            }
            Err(failure) => return Err(failure.browser_error()),
        };
        let route = BrowserRoute { provider };
        self.store_browser_route(key, route.clone()).await;
        Ok(route)
    }

    pub(super) async fn present_external_page(
        &self,
        identity: &BrowserAgentIdentity,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let key = route_key(identity, input);
        let session = session_name(identity);
        let mut target = match self.browser_routes.lock().await.get(&key) {
            Some(BrowserRoute {
                provider: BrowserRouteProvider::Opencli { target, .. },
            }) => target.clone(),
            _ => None,
        };
        target = requested_target(input).or(target);
        if input.get("url").is_some() {
            let opened = self
                .run_opencli_action(&key, &session, target.as_deref(), None, "open", input)
                .await
                .map_err(|failure| failure.browser_error())?;
            target = requested_target(&json!({ "tab_id": opened.get("browserTabId") }));
        }
        let target = target
            .ok_or_else(|| invalid_argument("Open an external browser tab before presenting it"))?;
        self.present_external_target(&key, &session, &target).await
    }

    async fn present_external_target(
        &self,
        key: &str,
        session: &str,
        target: &str,
    ) -> Result<Value, BrowserError> {
        let result = OpencliProvider::present(session, target)
            .await
            .map_err(|failure| failure.browser_error())?;
        self.store_browser_route(
            key,
            BrowserRoute {
                provider: BrowserRouteProvider::Opencli {
                    session: session.to_string(),
                    target: Some(target.to_string()),
                    display_tab: None,
                },
            },
        )
        .await;
        tracing::info!(target: "iyw_claw_browser", operation_provider = "opencli",
            display_provider = "opencli", "Presented the external browser tab");
        Ok(json!({
            "ok": true, "provider": "opencli",
            "browserTabId": opencli_browser_tab_id(session, Some(target)),
            "presentation": { "provider": "opencli", "status": "presented" },
            "output": result.output,
        }))
    }

    pub(super) async fn request_external_user_action(
        &self,
        identity: &BrowserAgentIdentity,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let mut presentation = input.clone();
        let session = session_name(identity);
        let key = route_key(identity, input);
        let route = self.browser_routes.lock().await.get(&key).cloned();
        let target = requested_target(input).or_else(|| match route?.provider {
            BrowserRouteProvider::Opencli { target, .. } => target,
            _ => None,
        });
        let target = match target {
            Some(target) => target,
            None => active_external_target(&session).await?,
        };
        if let Some(input) = presentation.as_object_mut() {
            input.remove("url");
            input.insert(
                "tab_id".to_string(),
                Value::String(opencli_browser_tab_id(&session, Some(&target))),
            );
        }
        self.present_external_page(identity, &presentation).await?;
        Err(OpencliFailure::user_action(
            "Complete the required user action in the external Chrome tab, then take a fresh snapshot before continuing. The built-in browser is disabled.",
        ).browser_error())
    }
}

async fn active_external_target(session: &str) -> Result<String, BrowserError> {
    let result = OpencliProvider::invoke(
        session,
        "tab",
        &["list".to_string()],
        None,
        EXTERNAL_STATE_TIMEOUT,
    )
    .await
    .map_err(|failure| failure.browser_error())?;
    result.output.as_array()
        .and_then(|tabs| tabs.iter().find(|tab| tab.get("active").and_then(Value::as_bool) == Some(true)))
        .and_then(|tab| tab.get("page").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| OpencliFailure::user_action(
            "Complete the required user action in external Chrome; no active OpenCLI tab was available to present.",
        ).browser_error())
}
