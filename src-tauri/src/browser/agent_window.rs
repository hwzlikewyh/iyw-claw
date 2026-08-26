use serde_json::{json, Value};

use super::agent_tool_cancellation::{ensure_request_active, AgentToolContext};
use super::agent_tool_support::{
    default_agent_tab_id, invalid_argument, optional_bool, optional_string, project_agent_state,
};
use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::types::{BrowserWindowCloseRequestSnapshot, BrowserWindowOpenRequestSnapshot};

#[derive(Debug, Clone)]
pub(super) struct PendingWindowClose {
    pub snapshot: BrowserWindowCloseRequestSnapshot,
}

#[derive(Debug, Clone)]
pub(super) struct PendingWindowOpen {
    pub snapshot: BrowserWindowOpenRequestSnapshot,
}

impl BrowserSessionManager {
    pub(super) async fn agent_present_window(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        let has_url = input.get("url").is_some();
        if !has_url && optional_bool(input, "new_tab")?.unwrap_or(false) {
            return Err(invalid_argument(
                "browser_present requires url when new_tab=true",
            ));
        }
        let open_result = if has_url {
            Some(self.agent_open(context, input).await?)
        } else {
            None
        };
        let state = self.agent_snapshot_for(context.identity).await;
        let tab_id = open_result
            .as_ref()
            .and_then(|value| value.get("targetTabId"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(optional_string(input, "tab_id", 128)?.map(str::to_string))
            .or_else(|| default_agent_tab_id(&state))
            .ok_or_else(|| invalid_argument("No managed browser tab is available"))?;
        if !state.tabs.iter().any(|tab| tab.browser_tab_id == tab_id) {
            return Err(BrowserError::tab_not_found(&tab_id));
        }
        self.agent_turn_leases.keep_tab_open(&tab_id).await;
        let request_id = self.request_window_open(&tab_id).await;
        Ok(project_agent_state(
            self.agent_snapshot_for(context.identity).await,
            Some(&tab_id),
            Some(json!({
                "status": "present_requested",
                "requestId": request_id,
            })),
        ))
    }

    pub(super) async fn agent_close_window(
        &self,
        context: AgentToolContext<'_>,
        input: &Value,
    ) -> Result<Value, BrowserError> {
        ensure_request_active(context)?;
        let requested_tab_id = optional_string(input, "tab_id", 128)
            .map_err(|_| invalid_argument("Invalid browser argument: tab_id"))?
            .map(str::to_string);
        let state = self.agent_snapshot_for(context.identity).await;
        let tab_id = requested_tab_id.or_else(|| default_agent_tab_id(&state));
        let Some(tab_id) = tab_id else {
            return Err(invalid_argument("No managed browser tab is available"));
        };
        let Some(tab) = state.tabs.iter().find(|tab| tab.browser_tab_id == tab_id) else {
            return Err(BrowserError::tab_not_found(&tab_id));
        };
        let has_detached_window = tab.host_id.as_ref().is_some_and(|host_id| {
            state.hosts.iter().any(|host| {
                host.host_id == *host_id && host.kind == super::types::BrowserHostKind::Detached
            })
        });
        if !has_detached_window {
            let pending_open = self
                .window_open_requests
                .lock()
                .await
                .values()
                .any(|request| request.snapshot.browser_tab_id == tab_id);
            if pending_open {
                self.cancel_window_open_requests(vec![tab_id.clone()]).await;
                return Ok(project_agent_state(
                    state,
                    Some(&tab_id),
                    Some(json!({
                        "status": "close_requested",
                        "cancelledPendingOpen": true,
                        "preservesTab": true,
                    })),
                ));
            }
            return Ok(project_agent_state(
                state,
                Some(&tab_id),
                Some(json!({ "status": "no_window" })),
            ));
        }
        let request_id = self.request_window_close(&tab_id).await;
        Ok(project_agent_state(
            state,
            Some(&tab_id),
            Some(json!({
                "status": "close_requested",
                "requestId": request_id,
                "preservesTab": true,
            })),
        ))
    }

    async fn request_window_close(&self, tab_id: &str) -> String {
        if let Some(request_id) = self
            .window_close_requests
            .lock()
            .await
            .values()
            .find(|request| request.snapshot.browser_tab_id == tab_id)
            .map(|request| request.snapshot.request_id.clone())
        {
            return request_id;
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        self.window_close_requests.lock().await.insert(
            request_id.clone(),
            PendingWindowClose {
                snapshot: BrowserWindowCloseRequestSnapshot {
                    request_id: request_id.clone(),
                    browser_tab_id: tab_id.to_string(),
                },
            },
        );
        request_id
    }

    async fn request_window_open(&self, tab_id: &str) -> String {
        if let Some(request_id) = self
            .window_open_requests
            .lock()
            .await
            .values()
            .find(|request| request.snapshot.browser_tab_id == tab_id)
            .map(|request| request.snapshot.request_id.clone())
        {
            return request_id;
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        self.window_open_requests.lock().await.insert(
            request_id.clone(),
            PendingWindowOpen {
                snapshot: BrowserWindowOpenRequestSnapshot {
                    request_id: request_id.clone(),
                    browser_tab_id: tab_id.to_string(),
                },
            },
        );
        request_id
    }

    pub(super) async fn cancel_window_close_requests(&self, tab_ids: Vec<String>) {
        let mut requests = self.window_close_requests.lock().await;
        requests.retain(|_, request| {
            tab_ids.is_empty() || !tab_ids.contains(&request.snapshot.browser_tab_id)
        });
    }

    pub(super) async fn cancel_window_open_requests(&self, tab_ids: Vec<String>) {
        let mut requests = self.window_open_requests.lock().await;
        requests.retain(|_, request| {
            tab_ids.is_empty() || !tab_ids.contains(&request.snapshot.browser_tab_id)
        });
    }

    pub async fn complete_window_open_request(
        &self,
        request_id: &str,
    ) -> super::types::BrowserStateSnapshot {
        self.window_open_requests.lock().await.remove(request_id);
        self.snapshot().await
    }
}
