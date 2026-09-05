use serde_json::{json, Value};

use super::agent_browser_input::{
    add_fallback, browser_action, managed_input, managed_semantic_command, requires_managed,
};
use super::agent_browser_request::{opencli_request, OpencliRequest};
use super::agent_browser_route::{
    ensure_provider_matches_input, input_requests_managed, opencli_route_from_input, route_key,
    session_name, validate_opencli_tab_session,
};
use super::agent_tool_cancellation::{ensure_request_active, AgentToolContext};
use super::agent_tool_support::invalid_argument;
use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::opencli::{OpencliFailure, OpencliProvider};
use super::types::BrowserAgentIdentity;
use crate::commands::internet_tools::allocate_opencli_screenshot_path;

#[derive(Debug, Clone)]
pub(super) enum BrowserRouteProvider {
    Opencli {
        session: String,
        target: Option<String>,
    },
    Managed {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(super) struct BrowserRoute {
    pub provider: BrowserRouteProvider,
}

impl BrowserSessionManager {
    pub(super) async fn agent_browser(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        let action = browser_action(input)?;
        if action == "list_tabs" {
            return self.agent_browser_list(context, input).await;
        }
        let key = route_key(context.identity, input);
        validate_opencli_tab_session(context.identity, input)?;
        let stored_route = self.browser_routes.lock().await.get(&key).cloned();
        ensure_provider_matches_input(stored_route.as_ref(), input)?;
        let route = stored_route.or_else(|| opencli_route_from_input(context.identity, input));
        if route.is_none() && input_requests_managed(input) {
            self.store_browser_route(
                &key,
                BrowserRoute {
                    provider: BrowserRouteProvider::Managed { reason: None },
                },
            )
            .await;
            return self.run_managed_action(context, &action, input, None).await;
        }
        if requires_managed(&action) {
            return self
                .run_provider_specific_action(context, &key, route, &action, input)
                .await;
        }
        let route = match route {
            Some(route) => route,
            None => self.start_opencli_route(&key, context.identity).await?,
        };
        self.run_routed_action(context, &key, route, &action, input)
            .await
    }

    async fn run_routed_action(
        &self,
        context: AgentToolContext<'_>,
        key: &str,
        route: BrowserRoute,
        action: &str,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        match route.provider {
            BrowserRouteProvider::Managed { reason } => {
                self.run_managed_action(context, action, input, reason.as_deref())
                    .await
            }
            BrowserRouteProvider::Opencli { session, target } => {
                match self
                    .run_opencli_action(key, &session, target.as_deref(), action, input)
                    .await
                {
                    Ok(value) => Ok(value),
                    Err(failure) if failure.is_user_action() => {
                        self.handoff_for_user_action(
                            context,
                            key,
                            &session,
                            target.as_deref(),
                            input,
                            &failure,
                        )
                        .await
                    }
                    Err(failure) => Err(failure.browser_error()),
                }
            }
        }
    }

    async fn start_opencli_route(
        &self,
        key: &str,
        identity: &BrowserAgentIdentity,
    ) -> Result<BrowserRoute, BrowserError> {
        if let Err(failure) = OpencliProvider::doctor().await {
            if failure.code == "OPENCLI_NOT_INSTALLED" {
                let route = BrowserRoute {
                    provider: BrowserRouteProvider::Managed { reason: None },
                };
                self.store_browser_route(key, route.clone()).await;
                return Ok(route);
            }
            return Err(failure.browser_error());
        }
        let route = BrowserRoute {
            provider: BrowserRouteProvider::Opencli {
                session: session_name(identity),
                target: None,
            },
        };
        self.store_browser_route(key, route.clone()).await;
        Ok(route)
    }

    async fn run_opencli_action(
        &self,
        key: &str,
        session: &str,
        current_target: Option<&str>,
        action: &str,
        input: &Value,
    ) -> Result<Value, OpencliFailure> {
        let (request, output_path) = prepare_opencli_request(action, input, current_target)?;
        let result = OpencliProvider::invoke(
            session,
            &request.command,
            &request.args,
            request.target.as_deref(),
            request.timeout,
        )
        .await?;
        enforce_opencli_screenshot_quota(output_path.as_deref()).await?;
        let next_target = result.target_id.or_else(|| request.target.clone());
        self.store_browser_route(
            key,
            BrowserRoute {
                provider: BrowserRouteProvider::Opencli {
                    session: session.to_string(),
                    target: next_target.clone(),
                },
            },
        )
        .await;
        Ok(json!({
            "ok": true,
            "provider": "opencli",
            "browserTabId": next_target
                .map(|target| format!("opencli:{session}:{target}"))
                .unwrap_or_else(|| format!("opencli:{session}")),
            "session": session,
            "url": input.get("url"),
            "path": output_path,
            "output": result.output,
        }))
    }

    pub(super) async fn run_managed_action(
        &self,
        context: AgentToolContext<'_>,
        action: &str,
        input: &Value,
        fallback_reason: Option<&str>,
    ) -> Result<Value, BrowserError> {
        let managed_input = managed_input(action, input)?;
        let semantic_input = managed_semantic_command(action, &managed_input)?;
        let value = match action {
            "open" => self.agent_open(context, &managed_input).await?,
            "snapshot" => self.agent_snapshot(context, &managed_input).await?,
            "read" => self.agent_read(context, &managed_input).await?,
            "click" => match semantic_input {
                Some(input) => self.agent_command(context, &input).await?,
                None => self.agent_click(context, &managed_input).await?,
            },
            "fill" => match semantic_input {
                Some(input) => self.agent_command(context, &input).await?,
                None => self.agent_fill(context, &managed_input).await?,
            },
            "press" => self.agent_press(context, &managed_input).await?,
            "scroll" => self.agent_scroll(context, &managed_input).await?,
            "wait" => self.agent_wait(context, &managed_input).await?,
            "screenshot" => self.agent_screenshot(context, &managed_input).await?,
            "close_tab" => self.agent_close(context, &managed_input).await?,
            "advanced" => self.agent_command(context, &managed_input).await?,
            "request_user_action" => {
                self.agent_request_user_action(context, &managed_input)
                    .await?
            }
            "present" => self.agent_present_window(context, &managed_input).await?,
            "close_window" => self.agent_close_window(context, &managed_input).await?,
            _ => return Err(invalid_argument("Unsupported browser action")),
        };
        Ok(add_fallback(value, fallback_reason))
    }

    pub(super) async fn store_browser_route(&self, key: &str, route: BrowserRoute) {
        const MAX_BROWSER_ROUTES: usize = 256;
        let mut routes = self.browser_routes.lock().await;
        if routes.len() >= MAX_BROWSER_ROUTES && !routes.contains_key(key) {
            if let Some(oldest) = routes.keys().next().cloned() {
                routes.remove(&oldest);
            }
        }
        routes.insert(key.to_string(), route);
    }
}

fn opencli_runtime_failure(message: impl Into<String>) -> OpencliFailure {
    OpencliFailure {
        code: "OPENCLI_RUNTIME_FAILED".to_string(),
        message: message.into(),
        kind: super::opencli::OpencliFailureKind::Runtime,
    }
}

fn prepare_opencli_request(
    action: &str,
    input: &Value,
    current_target: Option<&str>,
) -> Result<(OpencliRequest, Option<std::path::PathBuf>), OpencliFailure> {
    let mut request =
        opencli_request(action, input, current_target).map_err(|error| OpencliFailure {
            code: "OPENCLI_INVALID_ARGUMENT".to_string(),
            message: error.message,
            kind: super::opencli::OpencliFailureKind::Runtime,
        })?;
    let output_path = if action == "screenshot" {
        let path = allocate_opencli_screenshot_path().map_err(opencli_runtime_failure)?;
        request.args.insert(0, path.to_string_lossy().to_string());
        Some(path)
    } else {
        None
    };
    Ok((request, output_path))
}

async fn enforce_opencli_screenshot_quota(
    output_path: Option<&std::path::Path>,
) -> Result<(), OpencliFailure> {
    let Some(parent) = output_path.and_then(std::path::Path::parent) else {
        return Ok(());
    };
    super::screenshot_quota::enforce_screenshot_quota(parent)
        .await
        .map_err(|error| opencli_runtime_failure(error.message))
}
