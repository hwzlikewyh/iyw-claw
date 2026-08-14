use std::time::Duration;

use serde_json::Value;

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::records::RecoveryTab;
use super::runtime::BrowserRuntimeContext;
use super::tab_launch::{
    bind_existing_tab_preserving_target, cleanup_dead_tab_session, cleanup_tab, close_target_by_id,
    launch_tab, LaunchedTab,
};
use super::tabs::TabExitWatch;
use super::types::BrowserStateSnapshot;

const RECOVERY_ATTEMPTS: usize = 2;
const RECOVERY_RETRY_DELAY: Duration = Duration::from_millis(500);

struct RecoveryAttempt<'a> {
    tab_id: &'a str,
    runtime_generation: u64,
    number: usize,
}

struct RecoveryLaunch {
    launched: LaunchedTab,
    stale_target_id: Option<String>,
}

impl BrowserSessionManager {
    pub(super) fn spawn_tab_watcher(&self, watch: TabExitWatch) {
        let manager = self.clone();
        tokio::spawn(async move {
            let Some(exit) = watch.wait().await else {
                return;
            };
            manager.handle_tab_exit(exit).await;
        });
    }

    pub(super) fn schedule_tab_recovery(&self, tab_id: String, runtime_generation: u64) {
        let manager = self.clone();
        tokio::spawn(async move {
            for attempt in 0..RECOVERY_ATTEMPTS {
                if attempt > 0 {
                    tokio::time::sleep(RECOVERY_RETRY_DELAY).await;
                }
                let result = manager.restore_tab_once(&tab_id, runtime_generation).await;
                let context = RecoveryAttempt {
                    tab_id: &tab_id,
                    runtime_generation,
                    number: attempt + 1,
                };
                if log_recovery_result(context, &result) {
                    return;
                }
            }
        });
    }

    pub(super) async fn restore_browser_tab(
        &self,
        tab_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let generation = self.state.read().await.runtime.generation;
        self.restore_tab_once(tab_id, generation).await?;
        Ok(self.snapshot().await)
    }

    async fn handle_tab_exit(&self, exit: (String, String, u64)) {
        let (tab_id, session, runtime_generation) = exit;
        let Some(handle) = self.tabs.take_owned(&tab_id, &session).await else {
            return;
        };
        self.streams.close_tab(&tab_id).await;
        let accepted = self
            .state
            .write()
            .await
            .record_tab_crash(&tab_id, runtime_generation);
        if accepted {
            self.close_control(&tab_id).await;
        }
        cleanup_dead_tab_session(handle).await;
        if accepted {
            tracing::error!(
                target: "iyw_claw_browser",
                browser_tab_id = %tab_id,
                runtime_generation,
                "pinned browser tab daemon exited unexpectedly"
            );
            self.schedule_tab_recovery(tab_id, runtime_generation);
        }
    }

    async fn restore_tab_once(
        &self,
        tab_id: &str,
        runtime_generation: u64,
    ) -> Result<(), BrowserError> {
        let runtime = self.current_runtime(runtime_generation).await?;
        let tab = self.state.write().await.begin_tab_recovery(tab_id)?;
        self.streams.close_tab(tab_id).await;
        self.reset_control(tab_id).await;
        let result = match self.launch_recovery_tab(&runtime, &tab).await {
            Ok(launched) => self.register_recovered_tab(&tab, launched).await,
            Err(error) => Err(error),
        };
        if result.is_err() {
            self.fail_recovery_tab(&tab).await;
        }
        result
    }

    async fn current_runtime(
        &self,
        runtime_generation: u64,
    ) -> Result<BrowserRuntimeContext, BrowserError> {
        let context = match &self.runtime {
            Some(runtime) => runtime.context().await,
            None => None,
        };
        match context {
            Some(context) if context.generation == runtime_generation => Ok(context),
            _ => Err(runtime_changed()),
        }
    }

    async fn launch_recovery_tab(
        &self,
        runtime: &BrowserRuntimeContext,
        tab: &RecoveryTab,
    ) -> Result<RecoveryLaunch, BrowserError> {
        if let Some(target_id) = &tab.target_id {
            if let Ok(launched) =
                bind_existing_tab_preserving_target(runtime, &tab.ticket, target_id).await
            {
                return Ok(RecoveryLaunch {
                    launched,
                    stale_target_id: None,
                });
            }
            let launched = launch_tab(runtime, &tab.ticket, &tab.url).await?;
            return Ok(RecoveryLaunch {
                launched,
                stale_target_id: Some(target_id.clone()),
            });
        }
        Ok(RecoveryLaunch {
            launched: launch_tab(runtime, &tab.ticket, &tab.url).await?,
            stale_target_id: None,
        })
    }

    async fn register_recovered_tab(
        &self,
        tab: &RecoveryTab,
        recovery: RecoveryLaunch,
    ) -> Result<(), BrowserError> {
        let launched = recovery.launched;
        let target_id = launched.handle.target_id.clone();
        let watch = match self.tabs.insert(launched.handle).await {
            Ok(watch) => watch,
            Err(handle) => {
                let _ = cleanup_tab(handle, true).await;
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
            return Err(error);
        }
        self.spawn_tab_watcher(watch);
        if let Some(target_id) = recovery.stale_target_id {
            if let Some(runtime) = &self.runtime {
                if let Some(context) = runtime
                    .context()
                    .await
                    .filter(|context| context.generation == tab.ticket.runtime_generation)
                {
                    let _ =
                        close_target_by_id(&context.cli, &context.controller_session, &target_id)
                            .await;
                }
            }
        }
        Ok(())
    }

    pub(super) async fn finish_failed_navigation(
        &self,
        ticket: &super::records::TabTicket,
        error: &BrowserError,
    ) {
        if error.code != BrowserErrorCode::BrowserTabGone {
            let _ = self.state.write().await.fail_tab_navigation(ticket);
            return;
        }
        if self.state.write().await.mark_tab_gone(ticket).is_err() {
            return;
        }
        self.streams.close_tab(&ticket.tab_id).await;
        self.close_control(&ticket.tab_id).await;
        if let Some(handle) = self.tabs.take(&ticket.tab_id).await {
            let _ = cleanup_tab(handle, false).await;
        }
    }

    pub(super) async fn handle_target_failure(
        &self,
        generation: u64,
        params: Value,
        crashed: bool,
    ) {
        let Some(target_id) = params.get("targetId").and_then(Value::as_str) else {
            return;
        };
        let tab_id = self
            .state
            .write()
            .await
            .record_target_failure(target_id, crashed);
        let Some(tab_id) = tab_id else { return };
        self.streams.close_tab(&tab_id).await;
        self.close_control(&tab_id).await;
        if let Some(handle) = self.tabs.take(&tab_id).await {
            let _ = cleanup_tab(handle, false).await;
        }
        if crashed {
            self.schedule_tab_recovery(tab_id, generation);
        }
    }
}

fn log_recovery_result(context: RecoveryAttempt<'_>, result: &Result<(), BrowserError>) -> bool {
    match result {
        Ok(()) => tracing::info!(
            target: "iyw_claw_browser",
            browser_tab_id = %context.tab_id,
            runtime_generation = context.runtime_generation,
            attempt = context.number,
            "browser tab session recovered"
        ),
        Err(error) => tracing::warn!(
            target: "iyw_claw_browser",
            browser_tab_id = %context.tab_id,
            runtime_generation = context.runtime_generation,
            attempt = context.number,
            error_code = ?error.code,
            "browser tab session recovery failed"
        ),
    }
    match result {
        Ok(()) => true,
        Err(error) => !error.retryable,
    }
}

fn runtime_changed() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime changed during tab recovery",
    )
    .retryable(true)
}

fn recovery_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The recovered browser tab could not be registered",
    )
}
