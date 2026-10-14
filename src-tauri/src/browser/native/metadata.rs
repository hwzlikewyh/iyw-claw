use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use super::super::{BrowserSessionManager, BrowserTabStatus};

pub(super) fn publish(app: &AppHandle, id: &str) {
    let Ok(value) = super::state::snapshot(id) else {
        return;
    };
    let manager = app.state::<BrowserSessionManager>().inner().clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        manager.accept_native_metadata(&id, value).await;
    });
}

pub(super) fn message(app: &AppHandle, id: &str, value: Value) {
    match value["type"].as_str() {
        Some("error") => {
            let _ = super::state::with_page(id, |page| {
                page.errors = page.errors.saturating_add(1).min(99);
                Ok(())
            });
            publish(app, id);
        }
        Some("input") => {
            let suppressed = super::state::with_page(id, |page| {
                Ok(page
                    .agent_input_until
                    .is_some_and(|until| until > std::time::Instant::now()))
            })
            .unwrap_or(false);
            if suppressed {
                return;
            }
            let manager = app.state::<BrowserSessionManager>().inner().clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = manager.record_user_input(&id, true).await;
            });
        }
        Some("find") => {
            let _ = app.emit("browser://native-find", id);
        }
        _ => {}
    }
}

impl BrowserSessionManager {
    pub(crate) async fn refresh_native_tabs(&self) {
        if !self.is_native_browser().await {
            return;
        }
        let ids = self
            .state
            .read()
            .await
            .tabs
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            if let Ok(value) = self
                .native
                .command(&id, "state", serde_json::json!({}))
                .await
            {
                self.accept_native_metadata(&id, value).await;
            }
        }
    }

    pub(super) async fn accept_native_metadata(&self, id: &str, value: Value) {
        let mut state = self.state.write().await;
        let Some(tab) = state.tabs.get_mut(id) else {
            return;
        };
        if tab.target_id.as_deref() != Some(id) {
            return;
        }
        if let Some(url) = value["url"].as_str() {
            tab.url = url.to_string();
        }
        if let Some(title) = value["title"].as_str() {
            tab.title = title.to_string();
        }
        if let Some(epoch) = value["epoch"].as_u64() {
            tab.document_epoch = epoch;
        }
        tab.page_error_count = value["errors"].as_u64().unwrap_or(0) as u32;
        if matches!(
            tab.status,
            BrowserTabStatus::Live | BrowserTabStatus::Navigating
        ) {
            tab.status = if value["loading"] == true {
                BrowserTabStatus::Navigating
            } else {
                BrowserTabStatus::Live
            };
        }
    }
}
