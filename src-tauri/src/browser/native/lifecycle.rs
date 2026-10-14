use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::super::error::BrowserError;
use super::super::manager::BrowserSessionManager;
use super::super::records::RuntimeStartDecision;
use super::super::types::{BrowserRuntimeStatus, BrowserStateSnapshot};

impl BrowserSessionManager {
    pub(in crate::browser) async fn start_native_runtime(
        &self,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let _start = self.runtime_start_lock.lock().await;
        self.ensure_managed_browser_enabled()?;
        if self.state.read().await.runtime.status == BrowserRuntimeStatus::Running {
            return Ok(self.snapshot().await);
        }
        self.native
            .app
            .get()
            .ok_or_else(|| super::native_error("Native browser host is unavailable".into()))?;
        self.set_capability(super::super::engine_settings::lightweight_capability())
            .await;
        if let RuntimeStartDecision::Start(ticket) = self.begin_runtime_start().await? {
            self.complete_runtime_start(&ticket).await?;
            tracing::info!(
                runtime_generation = ticket.generation,
                engine = "webview",
                "native browser runtime ready"
            );
        }
        Ok(self.snapshot().await)
    }

    pub(in crate::browser) async fn create_native_tab(
        &self,
        url: String,
        host_id: Option<String>,
        cancellation: CancellationToken,
    ) -> Result<(BrowserStateSnapshot, String), BrowserError> {
        self.start_native_runtime().await?;
        if let Some(governor) = &self.resource_governor {
            governor.guard_new_tab()?;
        }
        let ticket = self.reserve_tab(url.clone(), host_id).await?;
        let created = self.prepare_native_page(&ticket, &url, cancellation).await;
        let value = match created {
            Ok(value) => value,
            Err(error) => {
                self.discard_native_tab(&ticket).await;
                return Err(error);
            }
        };
        let title = value["title"].as_str().unwrap_or("").to_string();
        let actual_url = value["url"].as_str().unwrap_or(&url).to_string();
        if let Err(error) = self
            .commit_tab_live(&ticket, ticket.tab_id.clone(), title, actual_url)
            .await
        {
            self.discard_native_tab(&ticket).await;
            return Err(error);
        }
        self.accept_native_metadata(&ticket.tab_id, value).await;
        Ok((self.snapshot().await, ticket.tab_id))
    }

    async fn discard_native_tab(&self, ticket: &super::super::records::TabTicket) {
        let _ = self
            .native
            .command(&ticket.tab_id, "close", json!({}))
            .await;
        let _ = self.rollback_tab(ticket).await;
        self.spawn_runtime_idle_check("native_tab_creation_failed");
    }

    async fn prepare_native_page(
        &self,
        ticket: &super::super::records::TabTicket,
        url: &str,
        cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        if cancellation.is_cancelled() {
            return Err(BrowserError::shutting_down());
        }
        self.native
            .command(&ticket.tab_id, "create", json!({"url":url}))
            .await?;
        self.native.restore_cookies(&ticket.tab_id).await?;
        self.sync_native_iyw_login(&ticket.tab_id).await;
        self.native
            .command(&ticket.tab_id, "navigate", json!({"url":url}))
            .await?;
        self.native
            .wait_loaded(&ticket.tab_id, url, cancellation)
            .await
            .map_err(|error| error.effect_may_have_occurred(true))
    }

    pub(in crate::browser) async fn run_native_navigation(
        &self,
        tab_id: &str,
        args: &[&str],
        cancellation: CancellationToken,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let ticket = self.state.write().await.begin_tab_navigation(tab_id)?;
        let (action, values) = match args.first().copied().unwrap_or("") {
            "open" => ("navigate", json!({"url":args.get(1)})),
            value => (value, json!({})),
        };
        let result = self
            .native
            .request(
                super::request::NativeRequest::new(tab_id, action, values)
                    .within(std::time::Duration::from_secs(30), cancellation.clone()),
            )
            .await;
        let result = match result {
            Ok(value) => {
                let destination = args
                    .get(1)
                    .copied()
                    .or_else(|| value["url"].as_str())
                    .unwrap_or("about:blank")
                    .to_string();
                self.native
                    .wait_loaded(tab_id, &destination, cancellation)
                    .await
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(value) => {
                let title = value["title"].as_str().unwrap_or("").to_string();
                let url = value["url"].as_str().unwrap_or("").to_string();
                self.state
                    .write()
                    .await
                    .finish_tab_navigation(&ticket, title, url)?;
                self.accept_native_metadata(tab_id, value).await;
                Ok(self.snapshot().await)
            }
            Err(error) => {
                self.finish_failed_navigation(&ticket, &error).await;
                Err(error)
            }
        }
    }

    pub async fn native_browser_view(&self, request: Value) -> Result<Value, BrowserError> {
        let tab_id = request["tabId"]
            .as_str()
            .ok_or_else(|| super::native_error("Browser tab is required".into()))?
            .to_string();
        self.agent_turn_leases.ensure_claimable(&tab_id).await?;
        if !self.is_native_browser().await {
            return Err(super::native_error("Native browser is not active".into()));
        }
        if !self.state.read().await.tabs.contains_key(&tab_id) {
            return Err(BrowserError::tab_not_found(&tab_id));
        }
        let state = self.state.read().await;
        let owner = request["owner"].as_str();
        let host = state
            .tabs
            .get(&tab_id)
            .and_then(|tab| tab.host_id.as_ref())
            .and_then(|id| state.hosts.get(id));
        if !host.is_some_and(|host| {
            Some(host.window_label.as_str()) == owner
                && (request["visible"] == false
                    || host.active_tab_id.as_deref() == Some(tab_id.as_str()))
        }) {
            return Err(super::native_error(
                "The native page belongs to a different browser host".into(),
            ));
        }
        drop(state);
        self.native.command(&tab_id, "view", request).await
    }

    pub async fn native_browser_action(
        &self,
        tab_id: &str,
        action: &str,
        values: Value,
    ) -> Result<Value, BrowserError> {
        if !matches!(action, "find" | "devtools" | "zoom" | "state" | "stop") {
            return Err(super::native_error(
                "Unsupported browser toolbar operation".into(),
            ));
        }
        if !self.is_native_browser().await {
            return Err(super::native_error("Native browser is not active".into()));
        }
        let lease = self.acquire_user_control(tab_id).await?;
        let result = self.native.command(tab_id, action, values).await;
        if let Ok(value) = &result {
            self.accept_native_metadata(tab_id, value.clone()).await;
        }
        lease.finish().await;
        result
    }
}
