use std::rc::Rc;

use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_runtime_wry::wry::{NewWindowFeatures, NewWindowResponse};

use super::super::manager::BrowserSessionManager;
use super::state::{NativePage, PAGES};

pub(super) fn open(
    app: &AppHandle,
    opener: &str,
    url: String,
    features: NewWindowFeatures,
) -> NewWindowResponse {
    if super::super::tab_actions::validated_url(&url).is_err() {
        return NewWindowResponse::Deny;
    }
    let owner_name = match super::state::with_page(opener, |page| Ok(page.owner.clone())) {
        Ok(owner) => owner,
        Err(_) => return NewWindowResponse::Deny,
    };
    let Some(owner) = app.get_webview_window(&owner_name) else {
        return NewWindowResponse::Deny;
    };
    let id = uuid::Uuid::new_v4().to_string();
    let view = match super::build::build(&owner, &id, Some(features.opener)) {
        Ok(view) => view,
        Err(error) => {
            tracing::warn!(error, "native browser popup creation failed");
            return NewWindowResponse::Deny;
        }
    };
    if super::download_control::install(&view, app, &id).is_err() {
        drop(view);
        #[cfg(target_os = "linux")]
        super::linux::remove(&id);
        return NewWindowResponse::Deny;
    }
    #[cfg(target_os = "windows")]
    let platform = tauri_runtime_wry::wry::WebViewExtWindows::webview(&view);
    #[cfg(target_os = "macos")]
    let platform =
        objc2::rc::Retained::into_super(tauri_runtime_wry::wry::WebViewExtMacOS::webview(&view));
    #[cfg(target_os = "linux")]
    let platform = tauri_runtime_wry::wry::WebViewExtUnix::webview(&view);
    PAGES.with(|pages| {
        pages
            .borrow_mut()
            .insert(id.clone(), NativePage::new(owner_name, Rc::new(view), url))
    });
    register(app, opener, id);
    NewWindowResponse::Create { webview: platform }
}

fn register(app: &AppHandle, opener: &str, id: String) {
    let manager = app.state::<BrowserSessionManager>().inner().clone();
    let opener = opener.to_string();
    tauri::async_runtime::spawn(async move {
        let result = manager.register_native_popup(&opener, &id).await;
        if let Err(error) = result {
            let _ = manager.native.command(&id, "close", json!({})).await;
            tracing::warn!(error_code = ?error.code, "native browser popup registration failed");
        }
    });
}

impl BrowserSessionManager {
    async fn register_native_popup(
        &self,
        opener: &str,
        id: &str,
    ) -> Result<(), super::super::error::BrowserError> {
        let _guard = self.tab_open_lock.lock().await;
        let host = self
            .state
            .read()
            .await
            .tabs
            .get(opener)
            .ok_or_else(|| super::super::error::BrowserError::tab_not_found(opener))?
            .host_id
            .clone();
        if let Some(governor) = &self.resource_governor {
            governor.guard_new_tab()?;
        }
        let metadata = self.native.command(id, "state", json!({})).await?;
        let url = metadata["url"]
            .as_str()
            .unwrap_or("about:blank")
            .to_string();
        let headless = host.is_none();
        let mut ticket = self.reserve_tab(url, host.clone()).await?;
        let reserved_id = ticket.tab_id.clone();
        if let Err(error) = self.rename_native_popup(&reserved_id, id, host).await {
            let _ = self.rollback_tab(&ticket).await;
            return Err(error);
        }
        ticket.tab_id = id.to_string();
        let result = self.commit_native_popup(&ticket, &metadata).await;
        if let Err(error) = result {
            let _ = self.rollback_tab(&ticket).await;
            return Err(error);
        }
        self.accept_native_metadata(id, metadata).await;
        self.agent_turn_leases.inherit_tab(opener, id).await;
        if headless {
            let close_now = self
                .agent_turn_leases
                .mark_close_pending(&[id.to_string()])
                .await;
            self.spawn_pending_tab_cleanup(close_now, "native_headless_popup");
        }
        Ok(())
    }

    async fn commit_native_popup(
        &self,
        ticket: &super::super::records::TabTicket,
        metadata: &serde_json::Value,
    ) -> Result<(), super::super::error::BrowserError> {
        self.commit_tab_live(
            ticket,
            ticket.tab_id.clone(),
            metadata["title"].as_str().unwrap_or("").to_string(),
            metadata["url"]
                .as_str()
                .unwrap_or("about:blank")
                .to_string(),
        )
        .await
    }

    async fn rename_native_popup(
        &self,
        reserved_id: &str,
        id: &str,
        host: Option<String>,
    ) -> Result<(), super::super::error::BrowserError> {
        let mut state = self.state.write().await;
        let mut tab = state
            .tabs
            .remove(reserved_id)
            .ok_or_else(|| super::super::error::BrowserError::tab_not_found(&reserved_id))?;
        tab.id = id.to_string();
        state.tabs.insert(id.to_string(), tab);
        if let Some(host) = host.and_then(|host| state.hosts.get_mut(&host)) {
            for value in &mut host.tab_order {
                if value == reserved_id {
                    *value = id.to_string();
                }
            }
            if host.active_tab_id.as_deref() == Some(reserved_id) {
                host.active_tab_id = Some(id.to_string());
            }
        }
        drop(state);
        self.controls.lock().await.remove(reserved_id);
        self.reset_control(id).await;
        Ok(())
    }
}
