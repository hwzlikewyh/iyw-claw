use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;

use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::records::{RecoveryPlan, RecoveryTab};
use super::runtime::{BrowserRuntime, BrowserRuntimeContext};
use super::tab_launch::{cleanup_tab, launch_tab};

const RECOVERY_ATTEMPTS: usize = 2;
const RECOVERY_RETRY_DELAY: Duration = Duration::from_millis(500);

impl BrowserSessionManager {
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
            let Some(plan) = self
                .state
                .write()
                .await
                .begin_runtime_recovery(failed_generation)
            else {
                return;
            };
            for tab in &plan.tabs {
                self.reset_control(&tab.ticket.tab_id).await;
            }
            match self
                .start_runtime_with_ticket(&runtime, plan.runtime.clone())
                .await
            {
                Ok(context) => {
                    self.recover_tabs(&context, &plan).await;
                    tracing::info!(
                        target: "iyw_claw_browser",
                        runtime_generation = context.generation,
                        restored_tabs = plan.tabs.len(),
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

    async fn recover_tabs(&self, runtime: &BrowserRuntimeContext, plan: &RecoveryPlan) {
        let tasks = plan.tabs.iter().cloned().map(|tab| {
            let manager = self.clone();
            let runtime = runtime.clone();
            async move { manager.recover_tab(&runtime, tab).await }
        });
        for result in join_all(tasks).await {
            if let Err(error) = result {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    error_code = ?error.code,
                    "logical browser tab could not be restored"
                );
            }
        }
    }

    async fn recover_tab(
        &self,
        runtime: &BrowserRuntimeContext,
        tab: RecoveryTab,
    ) -> Result<(), BrowserError> {
        let launched = match launch_tab(runtime, &tab.ticket, &tab.url).await {
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
                let _ = cleanup_tab(handle, true).await;
                self.fail_recovery_tab(&tab).await;
                return Err(recovery_error());
            }
        };
        if let Err(error) = self
            .commit_tab_live(&tab.ticket, target_id, launched.title, launched.url)
            .await
        {
            if let Some(handle) = self.tabs.take(&tab.ticket.tab_id).await {
                let _ = cleanup_tab(handle, true).await;
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
