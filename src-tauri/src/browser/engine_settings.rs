use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::types::{
    BrowserCapability, BrowserEngineKind, BrowserEngineSummary, BrowserRuntimeStatus,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEngineSettings {
    pub full_browser_enabled: bool,
    pub full_browser_installed: bool,
    pub active_engine: BrowserEngineKind,
    pub pending: bool,
    pub full_browser_error: Option<String>,
}

pub(super) fn lightweight_capability() -> BrowserCapability {
    BrowserCapability {
        supported: true,
        status: BrowserRuntimeStatus::Ready,
        reason: None,
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        sidecar_version: String::new(),
        sidecar_verified: false,
        engine: Some(BrowserEngineSummary {
            kind: BrowserEngineKind::Webview,
            version: "system".into(),
        }),
    }
}

fn full_installed() -> bool {
    crate::managed_environment::entrypoint("chromix", "chromix").is_some()
        && crate::managed_environment::entrypoint("agent-browser", "agent-browser").is_some()
}

impl BrowserSessionManager {
    pub(crate) fn with_app_handle(self, app: AppHandle) -> Self {
        let _ = self.native.app.set(app);
        self
    }

    pub async fn browser_engine_settings(&self) -> BrowserEngineSettings {
        let enabled = crate::preferences::load().full_browser_enabled;
        let installed = full_installed();
        let state = self.state.read().await;
        let active = state
            .capability
            .engine
            .as_ref()
            .map(|engine| engine.kind)
            .unwrap_or(BrowserEngineKind::Webview);
        let desired = if enabled && installed {
            BrowserEngineKind::Chromix
        } else {
            BrowserEngineKind::Webview
        };
        BrowserEngineSettings {
            full_browser_enabled: enabled,
            full_browser_installed: installed,
            active_engine: active,
            pending: active != desired && state.runtime.status == BrowserRuntimeStatus::Running,
            full_browser_error: self.native.full_error.lock().await.clone(),
        }
    }

    pub async fn set_full_browser_enabled(
        &self,
        app: &AppHandle,
        enabled: bool,
    ) -> Result<BrowserEngineSettings, BrowserError> {
        let _guard = self.tab_open_lock.lock().await;
        crate::preferences::update(|prefs| prefs.full_browser_enabled = enabled).map_err(
            |error| {
                BrowserError::new(
                    BrowserErrorCode::BrowserInternal,
                    format!("Could not save browser engine setting: {error}"),
                )
            },
        )?;
        if self.state.read().await.tabs.is_empty() && self.agent_turn_leases.is_empty().await {
            let running = self.state.read().await.runtime.status == BrowserRuntimeStatus::Running;
            if !running {
                self.apply_selected_capability().await;
            }
        }
        drop(_guard);
        self.stop_browser_runtime_if_idle("engine_setting_changed")
            .await;
        let _guard = self.tab_open_lock.lock().await;
        if self.state.read().await.runtime.status != BrowserRuntimeStatus::Running {
            self.apply_selected_capability().await;
        }
        let settings = self.publish_engine_settings(app).await;
        tracing::info!(enabled, pending = settings.pending, active_engine = ?settings.active_engine, "browser engine preference saved");
        Ok(settings)
    }

    async fn publish_engine_settings(&self, app: &AppHandle) -> BrowserEngineSettings {
        let settings = self.browser_engine_settings().await;
        if let Err(error) = app.emit("browser://engine-settings-changed", &settings) {
            tracing::warn!(%error, "browser engine setting broadcast failed");
        }
        settings
    }

    pub(super) async fn apply_selected_capability(&self) -> bool {
        let fixed = {
            let state = self.state.read().await;
            !state.tabs.is_empty()
                || matches!(
                    state.runtime.status,
                    BrowserRuntimeStatus::Starting
                        | BrowserRuntimeStatus::Running
                        | BrowserRuntimeStatus::Recovering
                        | BrowserRuntimeStatus::Stopping
                )
        };
        if fixed {
            return self.is_native_browser().await;
        }
        let capability = if crate::preferences::load().full_browser_enabled {
            match self.desktop_runtime() {
                Ok(runtime) => match runtime.verify().await {
                    Ok(capability) => {
                        *self.native.full_error.lock().await = None;
                        capability
                    }
                    Err(error) => {
                        *self.native.full_error.lock().await = Some(error.message.clone());
                        tracing::info!(error_code = ?error.code, "full browser is unavailable; using native browser");
                        lightweight_capability()
                    }
                },
                Err(_) => lightweight_capability(),
            }
        } else {
            *self.native.full_error.lock().await = None;
            lightweight_capability()
        };
        let native = capability
            .engine
            .as_ref()
            .is_some_and(|engine| engine.kind == BrowserEngineKind::Webview);
        self.set_capability(capability).await;
        native
    }

    pub(super) async fn is_native_browser(&self) -> bool {
        self.state
            .read()
            .await
            .capability
            .engine
            .as_ref()
            .is_some_and(|engine| engine.kind == BrowserEngineKind::Webview)
    }

    pub async fn browser_component_install_finished(
        &self,
        app: &AppHandle,
    ) -> Result<BrowserEngineSettings, BrowserError> {
        if let Some(runtime) = &self.runtime {
            runtime.invalidate_dependencies().await;
        }
        // 安装只刷新可用性，不回写偏好，避免覆盖下载期间用户的新选择。
        {
            let _guard = self.tab_open_lock.lock().await;
            self.apply_selected_capability().await;
        }
        self.stop_browser_runtime_if_idle("engine_install_finished")
            .await;
        Ok(self.publish_engine_settings(app).await)
    }
}
