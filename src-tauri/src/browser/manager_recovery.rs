use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;
use tokio_util::sync::CancellationToken;

use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::records::{RecoveryPlan, RecoveryTab};
use super::runtime::{BrowserRuntime, BrowserRuntimeContext};
use super::tab_launch::launch_tab;

const RECOVERY_ATTEMPTS: usize = 2;
const RECOVERY_RETRY_DELAY: Duration = Duration::from_millis(500);

impl BrowserSessionManager {
    pub(super) async fn cleanup_runtime_exit(
        &self,
        runtime: &BrowserRuntime,
        generation: u64,
    ) -> bool {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        if self.ensure_shutdown_epoch(epoch).is_err() {
            return false;
        }
        if !self.is_failed_runtime_generation(generation).await {
            return false;
        }
        self.stop_cdp_observer().await;
        self.streams.close_all().await;
        let tabs_result = self.shutdown_tabs().await;
        self.close_all_controls().await;
        let runtime_result = runtime.release_exited(generation).await;
        let tabs_result = if tabs_result.is_err() && runtime_result.is_ok() {
            self.finish_shutdown_tabs_after_runtime().await
        } else {
            tabs_result
        };
        let cleanup_error = tabs_result
            .as_ref()
            .err()
            .or_else(|| runtime_result.as_ref().err());
        if let Some(error) = cleanup_error {
            self.state
                .write()
                .await
                .record_runtime_cleanup_failure(generation, format!("{:?}", error.code));
        }
        tracing::error!(
            target: "iyw_claw_browser",
            runtime_generation = generation,
            tab_cleanup_error = tabs_result.as_ref().err().map(|error| error.message.as_str()),
            runtime_cleanup_error = runtime_result.as_ref().err().map(|error| error.message.as_str()),
            "browser controller exited unexpectedly"
        );
        runtime_result.is_ok()
    }

    pub(super) fn schedule_recovery(&self, runtime: Arc<BrowserRuntime>, failed_generation: u64) {
        let manager = self.clone();
        tokio::spawn(async move {
            manager
                .recover_after_failure(runtime, failed_generation)
                .await;
        });
    }

    pub(super) async fn recover_after_failure(
        &self,
        runtime: Arc<BrowserRuntime>,
        mut failed_generation: u64,
    ) {
        for attempt in 0..RECOVERY_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(RECOVERY_RETRY_DELAY).await;
            }
            let Some((plan, outcome)) = self
                .start_recovery_attempt(&runtime, failed_generation)
                .await
            else {
                return;
            };
            match outcome {
                Ok(context) => {
                    let restored_tabs = self.recover_tabs(&context, &plan).await;
                    tracing::info!(
                        target: "iyw_claw_browser",
                        runtime_generation = context.generation,
                        restored_tabs,
                        planned_tabs = plan.tabs.len(),
                        "browser runtime recovery completed"
                    );
                    return;
                }
                Err(error) => {
                    failed_generation = plan.runtime.generation;
                    self.state
                        .write()
                        .await
                        .fail_recovery_plan(&plan, format!("{:?}", error.code));
                    tracing::warn!(
                        target: "iyw_claw_browser",
                        attempt = attempt + 1,
                        error_code = ?error.code,
                        "browser runtime recovery attempt failed"
                    );
                }
            }
        }
    }

    async fn start_recovery_attempt(
        &self,
        runtime: &Arc<BrowserRuntime>,
        failed_generation: u64,
    ) -> Option<(RecoveryPlan, Result<BrowserRuntimeContext, BrowserError>)> {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        let _start_guard = self.runtime_start_lock.lock().await;
        self.ensure_shutdown_epoch(epoch).ok()?;
        let cancellation = self.shutdown_cancellation().await;
        let plan = self
            .state
            .write()
            .await
            .begin_runtime_recovery(failed_generation)?;
        for tab in &plan.tabs {
            self.reset_control(&tab.ticket.tab_id).await;
        }
        let outcome = self
            .start_runtime_with_ticket(runtime, plan.runtime.clone(), cancellation)
            .await;
        Some((plan, outcome))
    }

    async fn recover_tabs(&self, runtime: &BrowserRuntimeContext, plan: &RecoveryPlan) -> usize {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        if self.ensure_shutdown_epoch(epoch).is_err() {
            return 0;
        }
        let cancellation = self.shutdown_cancellation().await;
        let tasks = plan.tabs.iter().cloned().map(|tab| {
            let manager = self.clone();
            let runtime = runtime.clone();
            let cancellation = cancellation.clone();
            async move { manager.recover_tab(&runtime, tab, cancellation).await }
        });
        let mut restored = 0;
        for result in join_all(tasks).await {
            match result {
                Ok(()) => restored += 1,
                Err(error) => {
                    tracing::warn!(
                        target: "iyw_claw_browser",
                        error_code = ?error.code,
                        "logical browser tab could not be restored"
                    );
                }
            }
        }
        restored
    }

    async fn recover_tab(
        &self,
        runtime: &BrowserRuntimeContext,
        tab: RecoveryTab,
        cancellation: CancellationToken,
    ) -> Result<(), BrowserError> {
        let launched = match launch_tab(
            &self.tab_cleanups,
            runtime,
            &tab.ticket,
            &tab.url,
            cancellation,
        )
        .await
        {
            Ok(launched) => launched,
            Err(error) => {
                self.fail_recovery_tab(&tab).await;
                return Err(error);
            }
        };
        let target_id = launched.handle.target_id.clone();
        let watch = match self.tabs.insert(launched.handle).await {
            Ok(watch) => watch,
            Err(handle) => {
                let _ = self.cleanup_or_retain_tab_handle(handle, true).await;
                self.fail_recovery_tab(&tab).await;
                return Err(recovery_error());
            }
        };
        if let Err(error) = self
            .commit_tab_live(&tab.ticket, target_id, launched.title, launched.url)
            .await
        {
            if let Some(handle) = self.tabs.take(&tab.ticket.tab_id).await {
                let _ = self.cleanup_or_retain_tab_handle(handle, true).await;
            }
            self.fail_recovery_tab(&tab).await;
            return Err(error);
        }
        self.spawn_tab_watcher(watch);
        Ok(())
    }

    pub(super) async fn fail_recovery_tab(&self, tab: &RecoveryTab) {
        self.state.write().await.fail_recovery_tab(&tab.ticket);
        self.close_control(&tab.ticket.tab_id).await;
    }
}

fn recovery_error() -> BrowserError {
    BrowserError::new(
        super::error::BrowserErrorCode::BrowserInternal,
        "The recovered browser tab could not be registered",
    )
}
