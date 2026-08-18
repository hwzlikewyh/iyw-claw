use std::path::PathBuf;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::cdp_observer::CdpObserverHandle;
use super::error::{BrowserError, BrowserErrorCode};
use super::frame_protocol::ensure_frame_generations;
use super::manager::BrowserSessionManager;
use super::runtime::BrowserRuntimeContext;
use super::types::{BrowserGenerations, BrowserStateSnapshot};
use super::types_cdp::BrowserDownloadStatus;

impl BrowserSessionManager {
    pub(super) async fn start_cdp_observer(
        &self,
        runtime: &BrowserRuntimeContext,
        cancellation: CancellationToken,
    ) -> Result<(), BrowserError> {
        self.stop_cdp_observer().await;
        let observer = CdpObserverHandle::start(
            &runtime.cdp_url,
            &runtime.download_path,
            self.clone(),
            runtime.generation,
            cancellation,
        )
        .await?;
        *self.observer.lock().await = Some(observer);
        Ok(())
    }

    pub(super) async fn stop_cdp_observer(&self) {
        let observer = self.observer.lock().await.take();
        if let Some(observer) = observer {
            observer.stop().await;
        }
    }

    pub(super) async fn cancel_cdp_observer_without_wait(&self) {
        if let Some(observer) = self.observer.lock().await.take() {
            observer.cancel_without_wait().await;
        }
    }

    pub(super) async fn handle_cdp_disconnect(&self, generation: u64) {
        let accepted = self
            .record_runtime_exit(generation, "BROWSER_CDP_DISCONNECTED".to_string())
            .await;
        if !accepted {
            return;
        }
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        if self.ensure_shutdown_epoch(epoch).is_err() {
            return;
        }
        if !self.is_failed_runtime_generation(generation).await {
            return;
        }
        self.close_all_controls().await;
        self.streams.close_all().await;
        let tabs_result = self.shutdown_tabs().await;
        let mut recovered = false;
        if let Some(runtime) = &self.runtime {
            let runtime_result = runtime.stop().await;
            let tabs_result = if tabs_result.is_err() && runtime_result.is_ok() {
                self.finish_shutdown_tabs_after_runtime().await
            } else {
                tabs_result
            };
            self.observer.lock().await.take();
            if tabs_result.is_ok() && runtime_result.is_ok() {
                self.schedule_recovery(runtime.clone(), generation);
                recovered = true;
            }
        } else {
            self.observer.lock().await.take();
        }
        tracing::error!(
            target: "iyw_claw_browser",
            runtime_generation = generation,
            recovery_scheduled = recovered,
            "browser CDP observer disconnected unexpectedly"
        );
    }

    pub async fn answer_browser_dialog(
        &self,
        dialog_id: &str,
        expected: BrowserGenerations,
        accept: bool,
        prompt_text: Option<String>,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        if prompt_text.as_ref().is_some_and(|text| text.len() > 4096) {
            return Err(invalid_action("The browser prompt response is too long"));
        }
        let (session_id, actual) = self
            .state
            .read()
            .await
            .dialog_command(dialog_id)
            .ok_or_else(dialog_gone)?;
        ensure_frame_generations(&actual, &expected)?;
        self.cdp_call(
            "Page.handleJavaScriptDialog",
            json!({ "accept": accept, "promptText": prompt_text }),
            Some(session_id),
        )
        .await?;
        self.state.write().await.finish_dialog(dialog_id);
        Ok(self.snapshot().await)
    }

    pub async fn choose_browser_files(
        &self,
        chooser_id: &str,
        expected: BrowserGenerations,
        paths: Vec<String>,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let files = validate_upload_paths(paths)?;
        let (session_id, actual) = self
            .state
            .read()
            .await
            .chooser_command(chooser_id)
            .ok_or_else(chooser_gone)?;
        ensure_frame_generations(&actual, &expected)?;
        let action = if files.is_empty() { "cancel" } else { "accept" };
        self.cdp_call(
            "Page.handleFileChooser",
            json!({ "action": action, "files": files }),
            Some(session_id),
        )
        .await?;
        self.state.write().await.finish_chooser(chooser_id);
        Ok(self.snapshot().await)
    }

    pub async fn cancel_browser_download(
        &self,
        download_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        self.cdp_call(
            "Browser.cancelDownload",
            json!({ "guid": download_id }),
            None,
        )
        .await?;
        self.state.write().await.update_download(
            download_id,
            BrowserDownloadStatus::Cancelled,
            0,
            None,
            None,
        );
        Ok(self.snapshot().await)
    }

    pub async fn open_browser_download(&self, download_id: &str) -> Result<(), BrowserError> {
        let path = self.valid_download_path(download_id).await?;
        tauri_plugin_opener::open_path(path, None::<&str>).map_err(|_| download_unavailable())
    }

    pub async fn reveal_browser_download(&self, download_id: &str) -> Result<(), BrowserError> {
        let path = self.valid_download_path(download_id).await?;
        tauri_plugin_opener::reveal_item_in_dir(path).map_err(|_| download_unavailable())
    }

    async fn valid_download_path(&self, download_id: &str) -> Result<PathBuf, BrowserError> {
        let path = self
            .state
            .read()
            .await
            .completed_download_path(download_id)
            .ok_or_else(download_unavailable)?;
        let root = self
            .runtime
            .as_ref()
            .ok_or_else(download_unavailable)?
            .context()
            .await
            .ok_or_else(download_unavailable)?
            .download_path;
        let (root, path) =
            tokio::try_join!(tokio::fs::canonicalize(root), tokio::fs::canonicalize(path))
                .map_err(|_| download_unavailable())?;
        (path.starts_with(root) && path.is_file())
            .then_some(path)
            .ok_or_else(download_unavailable)
    }

    pub(super) async fn cdp_call(
        &self,
        method: &str,
        params: Value,
        session_id: Option<String>,
    ) -> Result<Value, BrowserError> {
        let observer = self
            .observer
            .lock()
            .await
            .clone()
            .ok_or_else(observer_gone)?;
        observer.call(method, params, session_id).await
    }
}

fn validate_upload_paths(paths: Vec<String>) -> Result<Vec<String>, BrowserError> {
    if paths.len() > 20 {
        return Err(invalid_action(
            "Too many browser upload files were selected",
        ));
    }
    paths
        .into_iter()
        .map(|value| {
            let path = PathBuf::from(&value);
            if !path.is_absolute() || !path.is_file() {
                return Err(invalid_action("A browser upload file is unavailable"));
            }
            Ok(path.to_string_lossy().into_owned())
        })
        .collect()
}

fn observer_gone() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser event observer is unavailable",
    )
    .retryable(true)
}

fn dialog_gone() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserDialogPending,
        "The browser dialog changed",
    )
}

fn chooser_gone() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserUploadCancelled,
        "The browser file chooser changed",
    )
}

fn invalid_action(message: &str) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserInternal, message)
}

fn download_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserDownloadFailed,
        "The completed browser download is unavailable",
    )
}
