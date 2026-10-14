//! 默认轻量浏览器；原生页面与 Agent 共用同一个视图。
mod actions;
mod agent;
mod build;
mod cdp;
mod cookies;
mod dispatch;
mod download_control;
mod downloads;
mod eval;
mod lifecycle;
#[cfg(target_os = "linux")]
mod linux;
mod metadata;
mod pointer;
mod popups;
mod profile;
mod request;
mod state;
mod window;
#[cfg(target_os = "windows")]
mod windows_cookies;

use std::sync::OnceLock;

use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::sync::oneshot;

use super::error::{BrowserError, BrowserErrorCode};
pub(in crate::browser) use request::NativeExecution;
use request::NativeRequest;

#[derive(Default)]
pub(super) struct NativeBrowser {
    pub(super) app: OnceLock<AppHandle>,
    pub(super) cookie_transfer:
        tokio::sync::Mutex<Option<(super::types::BrowserEngineKind, Vec<Value>)>>,
    pub(super) full_error: tokio::sync::Mutex<Option<String>>,
    pub(super) screenshot_lock: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for NativeBrowser {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output
            .debug_struct("NativeBrowser")
            .field("initialized", &self.app.get().is_some())
            .finish_non_exhaustive()
    }
}

impl NativeBrowser {
    pub(super) async fn command(
        &self,
        id: &str,
        action: &str,
        values: Value,
    ) -> Result<Value, BrowserError> {
        self.request(NativeRequest::new(id, action, values)).await
    }

    pub(super) async fn request(&self, request: NativeRequest<'_>) -> Result<Value, BrowserError> {
        let NativeRequest {
            id,
            action,
            values,
            timeout,
            cancellation,
        } = request;
        let app = self.app.get().ok_or_else(unavailable)?.clone();
        let operation = cancellation.child_token();
        let _cancel = operation.clone().drop_guard();
        let (sender, receiver) = oneshot::channel();
        let request = json!({"id": id, "action": action, "values": values});
        let handle = app.clone();
        let active = operation;
        app.run_on_main_thread(move || {
            if active.is_cancelled() || sender.is_closed() {
                return;
            }
            dispatch::execute(handle, request, active, sender);
        })
        .map_err(|error| native_error(error.to_string()))?;
        tokio::select! {
            _ = cancellation.cancelled() => Err(BrowserError::new(BrowserErrorCode::BrowserCancelled, "The browser operation was cancelled").effect_may_have_occurred(true)),
            result = tokio::time::timeout(timeout, receiver) => {
                result.map_err(|_| BrowserError::new(BrowserErrorCode::BrowserOperationTimeout, "The native browser operation timed out").effect_may_have_occurred(true))?
                    .map_err(|_| unavailable())?.map_err(native_error)
            }
        }
    }

    pub(super) async fn store_cookie_transfer(
        &self,
        source: super::types::BrowserEngineKind,
        value: &Value,
    ) {
        if let Some(cookies) = value["cookies"].as_array() {
            *self.cookie_transfer.lock().await = Some((source, cookies.clone()));
            tracing::info!(source_engine = ?source, cookie_count = cookies.len(), "browser cookie transfer prepared");
        }
    }
}

fn unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The native browser host is unavailable",
    )
}

pub(super) fn native_error(message: String) -> BrowserError {
    let code = if message.contains("STALE_REF") {
        BrowserErrorCode::BrowserSnapshotStale
    } else if message.contains("closed") {
        BrowserErrorCode::BrowserTabGone
    } else {
        BrowserErrorCode::BrowserInvalidArgument
    };
    BrowserError::new(code, message)
}
