use super::{BrowserRuntime, RuntimeHandle};
use crate::browser::error::BrowserError;
use crate::browser::process::kill_tree_checked;

impl BrowserRuntime {
    pub async fn stop(&self) -> Result<(), BrowserError> {
        let _mutation = self.mutation.lock().await;
        let current = { self.current.lock().await.take() };
        let current_result = match current {
            Some(handle) => {
                handle.watcher_cancel.cancel();
                let result = stop_handle(&handle).await;
                if result.is_err() {
                    *self.current.lock().await = Some(handle);
                }
                result
            }
            None => Ok(()),
        };
        let pending = { self.pending_cleanup.lock().await.take() };
        let pending_result = match pending {
            Some(mut cleanup) => {
                let result =
                    crate::browser::runtime_launch::cleanup_partial_owner(&mut cleanup).await;
                if result.is_err() {
                    *self.pending_cleanup.lock().await = Some(cleanup);
                }
                result
            }
            None => Ok(()),
        };
        current_result.and(pending_result)
    }

    pub async fn release_exited(&self, generation: u64) -> Result<(), BrowserError> {
        let _mutation = self.mutation.lock().await;
        let Some(handle) = self.take_generation(generation).await else {
            return Ok(());
        };
        handle.watcher_cancel.cancel();
        match force_stop_handle(&handle).await {
            Ok(()) => Ok(()),
            Err(error) => {
                tracing::error!(
                    target: "iyw_claw_browser",
                    runtime_generation = generation,
                    error_code = ?error.code,
                    "browser cleanup after controller exit remains incomplete"
                );
                *self.current.lock().await = Some(handle);
                Err(error)
            }
        }
    }

    async fn take_generation(&self, generation: u64) -> Option<RuntimeHandle> {
        let mut current = self.current.lock().await;
        (current.as_ref().map(|handle| handle.generation) == Some(generation))
            .then(|| current.take())
            .flatten()
    }
}

async fn stop_handle(handle: &RuntimeHandle) -> Result<(), BrowserError> {
    let initial_daemon_result = kill_tree_checked(&handle.daemon).await;
    if let Err(error) = initial_daemon_result {
        tracing::warn!(
            target: "iyw_claw_browser",
            error_code = ?error.code,
            "browser daemon required sweep retry"
        );
    }
    let (sidecar_result, engine_result) = tokio::join!(
        handle.cli.kill_sidecar_processes(),
        handle.cli.kill_profile_processes()
    );
    let daemon_result = kill_tree_checked(&handle.daemon).await;
    let cleanup_result = daemon_result.and(sidecar_result).and(engine_result);
    cleanup_result?;
    remove_runtime_dir(handle).await;
    Ok(())
}

async fn force_stop_handle(handle: &RuntimeHandle) -> Result<(), BrowserError> {
    let daemon_result = kill_tree_checked(&handle.daemon).await;
    let sidecar_result = handle.cli.kill_sidecar_processes().await;
    let engine_result = handle.cli.kill_profile_processes().await;
    daemon_result.and(sidecar_result).and(engine_result)?;
    remove_runtime_dir(handle).await;
    tracing::warn!(
        target: "iyw_claw_browser",
        "browser runtime required forced shutdown"
    );
    Ok(())
}

async fn remove_runtime_dir(handle: &RuntimeHandle) {
    let _ = tokio::fs::remove_dir_all(&handle.runtime_dir).await;
}
