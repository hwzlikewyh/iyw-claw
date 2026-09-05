use serde_json::{json, Map, Value};

use super::agent_browser::{BrowserRoute, BrowserRouteProvider};
use super::agent_browser_handoff_state::{
    capture_auth_state, current_url, site_origin, HandoffReport, HANDOFF_TTL_SECONDS,
};
use super::agent_tool_cancellation::AgentToolContext;
use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::opencli::OpencliFailure;

impl BrowserSessionManager {
    pub(super) async fn handoff_for_user_action(
        &self,
        context: AgentToolContext<'_>,
        key: &str,
        session: &str,
        target: Option<&str>,
        input: &Value,
        failure: &OpencliFailure,
    ) -> Result<Value, BrowserError> {
        self.store_browser_route(
            key,
            BrowserRoute {
                provider: BrowserRouteProvider::Managed {
                    reason: Some(failure.code.clone()),
                },
            },
        )
        .await;
        let url = current_url(session, target, input).await?;
        let mut report = capture_auth_state(session, target, &url).await;
        report.origin = site_origin(&url);
        let tab_id = self.create_handoff_tab(context).await?;
        report.cookies_imported = self.import_cookies(context, &tab_id, &report).await;
        self.agent_open(context, &json!({ "tab_id": tab_id, "url": url }))
            .await?;
        report.storage_imported = self.import_storage(context, &tab_id, &report).await;
        report.discard_sensitive_seeds();
        let user_action = self
            .request_handoff_user_action(context, &tab_id, input, failure)
            .await?;
        Ok(handoff_response(tab_id, failure, report, user_action))
    }

    async fn create_handoff_tab(
        &self,
        context: AgentToolContext<'_>,
    ) -> Result<String, BrowserError> {
        let opened = self
            .agent_open(context, &json!({ "url": "about:blank", "new_tab": true }))
            .await?;
        opened
            .get("targetTabId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                BrowserError::new(
                    super::error::BrowserErrorCode::BrowserInternal,
                    "Managed browser did not return the handoff tab id",
                )
            })
    }

    async fn import_cookies(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        report: &HandoffReport,
    ) -> usize {
        let mut imported = 0;
        for cookie in &report.cookies {
            let mut args = vec!["set".to_string(), cookie.name.clone(), cookie.value.clone()];
            append_cookie_args(&mut args, cookie);
            if self
                .agent_command(
                    context,
                    &json!({
                        "tab_id": tab_id,
                        "command": "cookies",
                        "arguments": args,
                    }),
                )
                .await
                .is_ok()
            {
                imported += 1;
            }
        }
        imported
    }

    async fn import_storage(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        report: &HandoffReport,
    ) -> usize {
        let mut imported = 0;
        for (storage_type, seeds) in [
            ("local", report.local_storage.as_slice()),
            ("session", report.session_storage.as_slice()),
        ] {
            for seed in seeds {
                let args = vec![
                    storage_type.to_string(),
                    "set".to_string(),
                    seed.key.clone(),
                    seed.value.clone(),
                ];
                if self
                    .agent_command(
                        context,
                        &json!({
                            "tab_id": tab_id,
                            "command": "storage",
                            "arguments": args,
                        }),
                    )
                    .await
                    .is_ok()
                {
                    imported += 1;
                }
            }
        }
        imported
    }

    async fn request_handoff_user_action(
        &self,
        context: AgentToolContext<'_>,
        tab_id: &str,
        input: &Value,
        failure: &OpencliFailure,
    ) -> Result<Value, BrowserError> {
        let mut request = Map::new();
        request.insert("tab_id".to_string(), Value::String(tab_id.to_string()));
        request.insert(
            "reason".to_string(),
            Value::String(format!(
                "OpenCLI requires user action ({}) in the managed browser",
                failure.code
            )),
        );
        if let Some(completion) = input.get("completion") {
            request.insert("completion".to_string(), completion.clone());
        }
        if let Some(timeout) = input
            .get("timeout_ms")
            .or_else(|| input.get("timeoutMs"))
            .cloned()
        {
            request.insert("timeout_ms".to_string(), timeout);
        }
        self.agent_request_user_action(context, &Value::Object(request))
            .await
    }
}

fn append_cookie_args(
    args: &mut Vec<String>,
    cookie: &super::agent_browser_handoff_state::CookieSeed,
) {
    if let Some(value) = &cookie.url {
        args.extend(["--url".to_string(), value.clone()]);
    }
    if let Some(value) = &cookie.domain {
        args.extend(["--domain".to_string(), value.clone()]);
    }
    if let Some(value) = &cookie.path {
        args.extend(["--path".to_string(), value.clone()]);
    }
    if cookie.http_only {
        args.push("--httpOnly".to_string());
    }
    if cookie.secure {
        args.push("--secure".to_string());
    }
    if let Some(value) = &cookie.same_site {
        args.extend(["--sameSite".to_string(), value.clone()]);
    }
    if let Some(value) = &cookie.expires {
        args.extend(["--expires".to_string(), value.clone()]);
    }
}

fn handoff_response(
    tab_id: String,
    failure: &OpencliFailure,
    report: HandoffReport,
    user_action: Value,
) -> Value {
    json!({
        "ok": true,
        "provider": "managed",
        "browserTabId": tab_id,
        "fallback": {
            "from": "opencli",
            "to": "managed",
            "reason": failure.code,
        },
        "authHandoff": {
            "status": handoff_status(&report),
            "origin": report.origin,
            "ttlSeconds": HANDOFF_TTL_SECONDS,
            "cookiesSeen": report.cookies_seen,
            "cookiesImported": report.cookies_imported,
            "storageImported": report.storage_imported,
            "credentialsCopied": false,
            "captureErrors": report.capture_errors,
        },
        "output": user_action,
    })
}

fn handoff_status(report: &HandoffReport) -> &'static str {
    if report.cookies_imported > 0 || report.storage_imported > 0 {
        "partial"
    } else {
        "manual_required"
    }
}
