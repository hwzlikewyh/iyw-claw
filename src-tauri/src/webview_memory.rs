use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewWindow};

const SUSPEND_DELAY: Duration = Duration::from_secs(30);
const PHASE_ACTIVE: u8 = 0;
const PHASE_PENDING: u8 = 1;
const PHASE_SUSPENDED: u8 = 2;

#[derive(Clone, Default)]
pub struct MainWebviewMemoryController {
    generation: Arc<AtomicU64>,
    phase: Arc<AtomicU8>,
}

pub fn note_hidden(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    if let Some(controller) = app.try_state::<MainWebviewMemoryController>() {
        controller.inner().clone().schedule_suspend(app.clone());
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app;
}

pub fn resume_before_show(window: &WebviewWindow) {
    #[cfg(target_os = "windows")]
    if let Some(controller) = window
        .app_handle()
        .try_state::<MainWebviewMemoryController>()
    {
        controller.inner().clone().resume(window.clone());
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

#[cfg(target_os = "windows")]
impl MainWebviewMemoryController {
    fn next_generation(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    fn schedule_suspend(self, app: AppHandle) {
        let generation = self.next_generation();
        tracing::debug!(generation, "main WebView hide grace period started");
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(SUSPEND_DELAY).await;
                if !self.suspend_if_idle(app.clone(), generation).await {
                    break;
                }
            }
        });
    }

    async fn suspend_if_idle(&self, app: AppHandle, generation: u64) -> bool {
        let Some(window) = app.get_webview_window("main") else {
            return false;
        };
        if !self.is_current(generation) || !window_is_hidden(&window) {
            return false;
        }
        if crate::desktop_shutdown::is_quitting() {
            return false;
        }
        if let Some(blocker) = suspend_blocker(&app).await {
            tracing::debug!(generation, blocker, "main WebView suspend skipped");
            return true;
        }
        if !self.begin_suspend(generation, &window) {
            return false;
        }
        self.dispatch_suspend(window, generation);
        false
    }

    fn begin_suspend(&self, generation: u64, window: &WebviewWindow) -> bool {
        self.is_current(generation)
            && window_is_hidden(window)
            && self
                .phase
                .compare_exchange(
                    PHASE_ACTIVE,
                    PHASE_PENDING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    fn dispatch_suspend(&self, window: WebviewWindow, generation: u64) {
        let controller = self.clone();
        let callback_window = window.clone();
        let result = window.with_webview(move |platform| {
            if !controller.is_current(generation) {
                controller.abort_pending();
                return;
            }
            if let Err(error) =
                windows_api::try_suspend(platform, controller.clone(), callback_window, generation)
            {
                controller.abort_pending();
                tracing::warn!(generation, error = %error, "main WebView TrySuspend failed");
            }
        });
        if let Err(error) = result {
            self.abort_pending();
            tracing::warn!(generation, error = %error, "main WebView dispatch failed");
        }
    }

    fn resume(&self, window: WebviewWindow) {
        let generation = self.next_generation();
        if self.phase.load(Ordering::Acquire) == PHASE_ACTIVE {
            return;
        }
        let controller = self.clone();
        if let Err(error) = window.with_webview(move |platform| {
            let result = windows_api::resume(platform);
            controller.record_resume(result.is_ok());
            match result {
                Ok(()) => tracing::info!(generation, "main WebView resumed"),
                Err(error) => {
                    tracing::warn!(generation, error = %error, "main WebView Resume failed")
                }
            }
        }) {
            tracing::warn!(generation, error = %error, "main WebView resume dispatch failed");
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    fn abort_pending(&self) {
        let _ = self.phase.compare_exchange(
            PHASE_PENDING,
            PHASE_ACTIVE,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn complete_suspend(&self, generation: u64, succeeded: bool, hidden: bool) -> bool {
        if !succeeded {
            self.abort_pending();
            return false;
        }
        if self.is_current(generation) && hidden {
            self.phase.store(PHASE_SUSPENDED, Ordering::Release);
            tracing::info!(generation, "main WebView suspended after hide grace period");
            return false;
        }
        true
    }

    fn record_resume(&self, succeeded: bool) {
        if succeeded {
            self.phase.store(PHASE_ACTIVE, Ordering::Release);
        } else if self.phase.load(Ordering::Acquire) != PHASE_ACTIVE {
            self.phase.store(PHASE_SUSPENDED, Ordering::Release);
        }
    }
}

#[cfg(target_os = "windows")]
async fn suspend_blocker(app: &AppHandle) -> Option<&'static str> {
    let Some(manager) = app.try_state::<crate::acp::manager::ConnectionManager>() else {
        return Some("agent_state_unavailable");
    };
    if manager.has_active_agent_operations().await {
        return Some("agent_operation_active");
    }
    let Some(voice) = app.try_state::<crate::commands::realtime_voice::RealtimeVoiceState>() else {
        return Some("voice_state_unavailable");
    };
    voice.has_session("main").await.then_some("voice_active")
}

#[cfg(target_os = "windows")]
fn window_is_hidden(window: &WebviewWindow) -> bool {
    window.is_visible().is_ok_and(|visible| !visible)
}

#[cfg(target_os = "windows")]
mod windows_api {
    use super::{window_is_hidden, MainWebviewMemoryController};
    use tauri::webview::PlatformWebview;
    use tauri::WebviewWindow;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use webview2_com::TrySuspendCompletedHandler;
    use windows_core::Interface;

    pub(super) fn try_suspend(
        platform: PlatformWebview,
        controller: MainWebviewMemoryController,
        window: WebviewWindow,
        generation: u64,
    ) -> Result<(), String> {
        let webview = core_webview(platform)?;
        let callback_webview = webview.clone();
        let callback = TrySuspendCompletedHandler::create(Box::new(move |status, succeeded| {
            let operation_ok = status.map(|_| true).unwrap_or_else(|error| {
                tracing::warn!(generation, error = %error, "main WebView suspend callback failed");
                false
            });
            let should_resume = controller.complete_suspend(
                generation,
                operation_ok && succeeded,
                window_is_hidden(&window),
            );
            if should_resume {
                let resume_result = resume_core(&callback_webview);
                let resumed = resume_result.is_ok();
                controller.record_resume(resumed);
                if let Err(error) = resume_result {
                    tracing::warn!(
                        generation,
                        error = %error,
                        "stale WebView suspend could not be resumed"
                    );
                }
            }
            Ok(())
        }));
        unsafe { webview.TrySuspend(&callback) }.map_err(|error| error.to_string())
    }

    pub(super) fn resume(platform: PlatformWebview) -> Result<(), String> {
        resume_core(&core_webview(platform)?)
    }

    fn core_webview(platform: PlatformWebview) -> Result<ICoreWebView2_3, String> {
        let core =
            unsafe { platform.controller().CoreWebView2() }.map_err(|error| error.to_string())?;
        core.cast::<ICoreWebView2_3>()
            .map_err(|error| error.to_string())
    }

    fn resume_core(webview: &ICoreWebView2_3) -> Result<(), String> {
        unsafe { webview.Resume() }.map_err(|error| error.to_string())
    }
}
