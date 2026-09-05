use std::time::Duration;

use serde_json::{json, Value};

use super::agent_browser::{BrowserRoute, BrowserRouteProvider};
use super::agent_browser_route::{route_key, session_name};
use super::agent_tool_cancellation::AgentToolContext;
use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::opencli::{OpencliFailure, OpencliProvider};

impl BrowserSessionManager {
    pub(super) async fn agent_browser_list(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        let managed = managed_state(self, context).await;
        let session = session_name(context.identity);
        let opencli = opencli_state(&session).await;
        let active_provider = self
            .active_browser_provider(context, input, opencli.0)
            .await;
        let tabs = combined_tabs(&opencli.2, &managed);
        let active_tab_id = active_tab_id(active_provider, &opencli.2, &managed);
        Ok(json!({
            "ok": true,
            "provider": active_provider,
            "activeTabId": active_tab_id,
            "tabs": tabs,
            "providers": {
                "opencli": {
                    "doctor": opencli.1,
                    "tabs": opencli.2,
                },
                "managed": managed,
            },
            "activeProvider": active_provider,
        }))
    }

    pub(super) async fn run_provider_specific_action(
        &self,
        context: AgentToolContext<'_>,
        key: &str,
        route: Option<BrowserRoute>,
        action: &str,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        if let Some(BrowserRoute {
            provider: BrowserRouteProvider::Opencli { session, target },
        }) = route
        {
            if action == "request_user_action" {
                let failure = OpencliFailure::user_action(
                    input
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("OpenCLI requires user interaction"),
                );
                return self
                    .handoff_for_user_action(
                        context,
                        key,
                        &session,
                        target.as_deref(),
                        input,
                        &failure,
                    )
                    .await;
            }
            return Ok(opencli_window_response(action, &session, target.as_deref()));
        }
        self.store_browser_route(
            key,
            BrowserRoute {
                provider: BrowserRouteProvider::Managed { reason: None },
            },
        )
        .await;
        self.run_managed_action(context, action, input, None).await
    }

    async fn active_browser_provider(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
        _opencli_ready: bool,
    ) -> &'static str {
        match self
            .browser_routes
            .lock()
            .await
            .get(&route_key(context.identity, input))
            .map(|route| &route.provider)
        {
            Some(BrowserRouteProvider::Opencli { .. }) => "opencli",
            Some(BrowserRouteProvider::Managed { .. }) => "managed",
            None => "managed",
        }
    }
}

async fn managed_state(manager: &BrowserSessionManager, context: AgentToolContext<'_>) -> Value {
    let mut value = manager
        .agent_state(context, None, None)
        .await
        .unwrap_or_else(|error| json!({ "error": error }));
    if let Some(map) = value.as_object_mut() {
        map.insert("provider".to_string(), Value::String("managed".to_string()));
    }
    value
}

async fn opencli_state(session: &str) -> (bool, Value, Value) {
    let doctor = match OpencliProvider::doctor().await {
        Ok(value) => value,
        Err(failure) => json!({
            "provider": "opencli",
            "status": "unavailable",
            "error": failure.browser_error(),
        }),
    };
    let ready = doctor.get("status").and_then(Value::as_str) == Some("ready");
    let tabs = if ready {
        OpencliProvider::invoke(
            session,
            "tab",
            &["list".to_string()],
            None,
            Duration::from_secs(30),
        )
        .await
        .map(|result| project_opencli_tabs(session, result.output))
        .unwrap_or_else(|failure| json!({ "error": failure.browser_error() }))
    } else {
        json!({ "error": "OPENCLI_UNAVAILABLE" })
    };
    (ready, doctor, tabs)
}

fn project_opencli_tabs(session: &str, value: Value) -> Value {
    let Value::Array(tabs) = value else {
        return value;
    };
    Value::Array(
        tabs.into_iter()
            .map(|mut tab| {
                if let Some(map) = tab.as_object_mut() {
                    if let Some(page) = map.get("page").and_then(Value::as_str) {
                        map.insert(
                            "browserTabId".to_string(),
                            Value::String(format!("opencli:{session}:{page}")),
                        );
                    }
                    map.insert("provider".to_string(), Value::String("opencli".to_string()));
                }
                tab
            })
            .collect(),
    )
}

fn opencli_window_response(action: &str, session: &str, target: Option<&str>) -> Value {
    json!({
        "ok": true,
        "provider": "opencli",
        "browserTabId": target
            .map(|target| format!("opencli:{session}:{target}"))
            .unwrap_or_else(|| format!("opencli:{session}")),
        "output": {
            "status": if action == "present" {
                "already_user_visible"
            } else {
                "user_window_not_managed"
            },
            "preservesTab": true,
        },
    })
}

fn combined_tabs(opencli: &Value, managed: &Value) -> Vec<Value> {
    let mut tabs = opencli.as_array().cloned().unwrap_or_default();
    tabs.extend(
        managed
            .get("tabs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .map(|mut tab| {
                if let Some(map) = tab.as_object_mut() {
                    map.insert("provider".to_string(), Value::String("managed".to_string()));
                }
                tab
            }),
    );
    tabs
}

fn active_tab_id(provider: &str, opencli: &Value, managed: &Value) -> Option<String> {
    if provider == "opencli" {
        return opencli.as_array().and_then(|tabs| {
            tabs.iter().find_map(|tab| {
                (tab.get("active").and_then(Value::as_bool) == Some(true))
                    .then(|| tab.get("browserTabId").and_then(Value::as_str))
                    .flatten()
                    .map(str::to_string)
            })
        });
    }
    managed
        .get("activeTabId")
        .and_then(Value::as_str)
        .map(str::to_string)
}
