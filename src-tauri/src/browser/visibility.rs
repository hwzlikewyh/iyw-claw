use std::sync::atomic::Ordering;

use serde::Deserialize;
use tauri::Emitter;

use super::{BrowserError, BrowserErrorCode, BrowserSessionManager};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisibilityUpdate {
    pub visible: bool,
    #[serde(default)]
    pub initialize_only: bool,
}

impl BrowserSessionManager {
    pub async fn update_browser_visibility(
        &self,
        app: &tauri::AppHandle,
        request: BrowserVisibilityUpdate,
    ) -> Result<bool, BrowserError> {
        let _guard = self.browser_visibility_lock.lock().await;
        let stored = crate::preferences::load().builtin_browser_enabled;
        let visible = if request.initialize_only {
            stored.unwrap_or(request.visible)
        } else {
            request.visible
        };
        if stored != Some(visible) {
            persist_visibility(visible)?;
        }
        self.managed_browser_enabled
            .store(visible, Ordering::Release);
        if !visible {
            self.cancel_user_action_requests(Vec::new()).await;
            self.window_open_requests.lock().await.clear();
        }
        if stored != Some(visible) {
            tracing::info!(target: "iyw_claw_browser", visible,
                initialized = request.initialize_only, "Built-in browser routing setting saved");
            if let Err(error) = app.emit("browser://visibility-changed", visible) {
                tracing::warn!(target: "iyw_claw_browser", %error,
                    "Failed to broadcast built-in browser setting");
            }
        }
        Ok(visible)
    }

    pub(super) fn managed_browser_enabled(&self) -> bool {
        self.managed_browser_enabled.load(Ordering::Acquire)
    }

    pub(super) fn ensure_managed_browser_enabled(&self) -> Result<(), BrowserError> {
        if self.managed_browser_enabled() {
            return Ok(());
        }
        Err(BrowserError::new(
            BrowserErrorCode::BrowserControlChanged,
            "The built-in browser is disabled. Use browser(action=list_tabs), open an external browser tab, and take a fresh snapshot before continuing.",
        ))
    }
}

fn persist_visibility(visible: bool) -> Result<(), BrowserError> {
    crate::preferences::update(|prefs| prefs.builtin_browser_enabled = Some(visible)).map_err(
        |error| {
            tracing::error!(target: "iyw_claw_browser", %error, visible,
                "Failed to persist built-in browser setting");
            BrowserError::new(
                BrowserErrorCode::BrowserInternal,
                "Failed to save the built-in browser setting",
            )
        },
    )
}
