use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::records::{RuntimeStartDecision, RuntimeTicket};
use super::runtime::{BrowserRuntime, BrowserRuntimeContext};
use super::state::BrowserState;
use super::types::{BrowserCapability, BrowserRuntimeStatus, BrowserStateSnapshot};

impl BrowserSessionManager {
    pub fn new_desktop(data_root: PathBuf) -> Self {
        let runtime = Arc::new(BrowserRuntime::new(data_root));
        Self::with_runtime(BrowserRuntime::initial_capability(), Some(runtime))
    }

    pub async fn refresh_capability(&self) -> BrowserStateSnapshot {
        let capability = match self.desktop_runtime() {
            Ok(runtime) => match runtime.verify().await {
                Ok(capability) => capability,
                Err(error) => capability_from_error(&error),
            },
            Err(error) => capability_from_error(&error),
        };
        self.set_capability(capability).await;
        self.snapshot().await
    }

    pub async fn start_browser_runtime(&self) -> Result<BrowserStateSnapshot, BrowserError> {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        let cancellation = self.shutdown_cancellation().await;
        self.ensure_runtime_running(cancellation).await?;
        Ok(self.snapshot().await)
    }

    pub async fn stop_browser_runtime(&self) -> Result<BrowserStateSnapshot, BrowserError> {
        self.stop_browser_runtime_with(|| async { Ok(()) }).await
    }

    pub async fn shutdown(&self) -> Result<(), BrowserError> {
        self.stop_browser_runtime().await.map(|_| ())
    }

    pub async fn stop_browser_runtime_with<F, Fut>(
        &self,
        finalize: F,
    ) -> Result<BrowserStateSnapshot, BrowserError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), BrowserError>>,
    {
        let _shutdown_guard = self.shutdown_lock.lock().await;
        let shutdown_epoch = self.begin_shutdown().await;
        let runtime_result = self.shutdown_resources().await;
        let finalize_result = finalize().await;
        self.finish_shutdown(shutdown_epoch).await;
        let result = runtime_result.and(finalize_result);
        log_shutdown_result(shutdown_epoch, &result);
        result?;
        Ok(self.snapshot().await)
    }

    pub(super) async fn ensure_runtime_running(
        &self,
        cancellation: CancellationToken,
    ) -> Result<BrowserRuntimeContext, BrowserError> {
        let _start_guard = self.runtime_start_lock.lock().await;
        let runtime = self.desktop_runtime()?;
        let status = self.state.read().await.runtime.status;
        if status == BrowserRuntimeStatus::Running {
            return runtime.context().await.ok_or_else(runtime_state_mismatch);
        }
        if status == BrowserRuntimeStatus::Failed {
            self.retry_failed_cleanup_before_start(runtime).await?;
        }
        if !self.tabs.is_empty().await {
            return Err(incomplete_tab_cleanup_error());
        }
        let capability = runtime.verify().await?;
        self.set_capability(capability).await;
        match self.begin_runtime_start().await? {
            RuntimeStartDecision::AlreadyRunning => {
                runtime.context().await.ok_or_else(runtime_state_mismatch)
            }
            RuntimeStartDecision::Start(ticket) => {
                self.start_runtime_with_ticket(runtime, ticket, cancellation)
                    .await
            }
        }
    }

    pub(super) async fn begin_shutdown(&self) -> u64 {
        let shutdown_epoch = self
            .shutdown_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1);
        self.shutdown_cancellation.lock().await.cancel();
        tracing::info!(
            target: "iyw_claw_browser",
            shutdown_epoch,
            "browser shutdown started"
        );
        shutdown_epoch
    }

    pub(super) async fn finish_shutdown(&self, shutdown_epoch: u64) {
        *self.shutdown_cancellation.lock().await = CancellationToken::new();
        let finished_epoch = self
            .shutdown_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1);
        debug_assert_eq!(finished_epoch, shutdown_epoch.saturating_add(1));
    }

    async fn shutdown_resources(&self) -> Result<(), BrowserError> {
        self.close_all_controls().await;
        self.stop_cdp_observer().await;
        let _tab_guard = self.tab_open_lock.lock().await;
        let _start_guard = self.runtime_start_lock.lock().await;
        self.stop_runtime_state_and_resources().await
    }

    pub(super) async fn stop_runtime_state_and_resources(&self) -> Result<(), BrowserError> {
        let Some(runtime) = &self.runtime else {
            self.agent_turn_leases.clear().await;
            return Ok(());
        };
        let state_transitioned = self.state.write().await.begin_runtime_stop();
        let result = self.stop_runtime_resources(runtime).await;
        if state_transitioned {
            let failure_code = result
                .as_ref()
                .err()
                .map(|error| format!("{:?}", error.code));
            self.state.write().await.finish_runtime_stop(failure_code);
        }
        self.agent_turn_leases.clear().await;
        result
    }

    async fn stop_runtime_resources(&self, runtime: &BrowserRuntime) -> Result<(), BrowserError> {
        self.stop_cdp_observer().await;
        self.stop_runtime_owners(runtime).await
    }

    async fn stop_runtime_owners(&self, runtime: &BrowserRuntime) -> Result<(), BrowserError> {
        let (_, tabs_result) = tokio::join!(self.streams.close_all(), self.shutdown_tabs());
        let runtime_result = runtime.stop().await;
        let tabs_result = if tabs_result.is_err() && runtime_result.is_ok() {
            self.finish_shutdown_tabs_after_runtime().await
        } else {
            tabs_result
        };
        tabs_result.and(runtime_result)
    }

    async fn retry_failed_cleanup_before_start(
        &self,
        runtime: &BrowserRuntime,
    ) -> Result<(), BrowserError> {
        self.cancel_cdp_observer_without_wait().await;
        let result = self.stop_runtime_owners(runtime).await;
        let failure_code = result
            .as_ref()
            .err()
            .map(|error| format!("{:?}", error.code));
        self.state.write().await.finish_runtime_stop(failure_code);
        result
    }

    pub(super) async fn is_failed_runtime_generation(&self, generation: u64) -> bool {
        let state = self.state.read().await;
        state.runtime.generation == generation
            && state.runtime.status == BrowserRuntimeStatus::Failed
    }

    pub(super) fn with_runtime(
        capability: BrowserCapability,
        runtime: Option<Arc<BrowserRuntime>>,
    ) -> Self {
        Self {
            state: Arc::new(tokio::sync::RwLock::new(BrowserState::new(capability))),
            controls: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            snapshot_revision: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            shutdown_lock: Arc::new(tokio::sync::Mutex::new(())),
            shutdown_cancellation: Arc::new(tokio::sync::Mutex::new(
                tokio_util::sync::CancellationToken::new(),
            )),
            runtime_start_lock: Arc::new(tokio::sync::Mutex::new(())),
            tab_open_lock: Arc::new(tokio::sync::Mutex::new(())),
            shutdown_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            runtime,
            tabs: Arc::new(super::tabs::BrowserTabRegistry::default()),
            tab_cleanups: Arc::new(
                super::tab_cleanup_registry::PendingTabCleanupRegistry::default(),
            ),
            streams: Arc::new(super::stream::BrowserStreamRegistry::default()),
            observer: Arc::new(tokio::sync::Mutex::new(None)),
            agent_turn_leases: Arc::new(super::agent_turn_leases::AgentTurnLeaseRegistry::default()),
        }
    }

    pub(super) fn desktop_runtime(&self) -> Result<&Arc<BrowserRuntime>, BrowserError> {
        self.runtime.as_ref().ok_or_else(|| {
            BrowserError::new(
                BrowserErrorCode::BrowserUnsupportedRuntime,
                "The shared browser is unavailable in this runtime",
            )
        })
    }

    pub(super) async fn start_runtime_with_ticket(
        &self,
        runtime: &Arc<BrowserRuntime>,
        ticket: RuntimeTicket,
        cancellation: CancellationToken,
    ) -> Result<BrowserRuntimeContext, BrowserError> {
        match runtime.start(ticket.generation, cancellation.clone()).await {
            Ok(context) => match self
                .start_cdp_observer(&context, cancellation.clone())
                .await
            {
                Err(error) => {
                    let _ = runtime.stop().await;
                    let _ = self
                        .fail_runtime_start(&ticket, format!("{:?}", error.code))
                        .await;
                    Err(error)
                }
                Ok(()) if cancellation.is_cancelled() => {
                    self.stop_cdp_observer().await;
                    let _ = runtime.stop().await;
                    let error = BrowserError::shutting_down();
                    let _ = self
                        .fail_runtime_start(&ticket, format!("{:?}", error.code))
                        .await;
                    Err(error)
                }
                Ok(()) => match self.complete_runtime_start(&ticket).await {
                    Ok(()) => {
                        self.spawn_runtime_watcher(Arc::clone(runtime), context.generation)
                            .await;
                        Ok(context)
                    }
                    Err(error) => {
                        self.stop_cdp_observer().await;
                        let _ = runtime.stop().await;
                        Err(error)
                    }
                },
            },
            Err(error) => {
                let _ = self
                    .fail_runtime_start(&ticket, format!("{:?}", error.code))
                    .await;
                Err(error)
            }
        }
    }

    async fn spawn_runtime_watcher(&self, runtime: Arc<BrowserRuntime>, generation: u64) {
        let Some(watch) = runtime.take_exit_watch(generation).await else {
            return;
        };
        let manager = self.clone();
        tokio::spawn(async move {
            let Some(exited_generation) = watch.wait().await else {
                return;
            };
            let accepted = manager
                .record_runtime_exit(exited_generation, "BROWSER_RUNTIME_UNAVAILABLE".to_string())
                .await;
            if accepted
                && manager
                    .cleanup_runtime_exit(&runtime, exited_generation)
                    .await
            {
                manager.schedule_recovery(runtime, exited_generation);
            }
        });
    }
}

fn log_shutdown_result(shutdown_epoch: u64, result: &Result<(), BrowserError>) {
    match result {
        Ok(()) => tracing::info!(
            target: "iyw_claw_browser",
            shutdown_epoch,
            "browser shutdown completed"
        ),
        Err(error) => tracing::error!(
            target: "iyw_claw_browser",
            shutdown_epoch,
            error_code = ?error.code,
            error = %error,
            "browser shutdown failed"
        ),
    }
}

fn capability_from_error(error: &BrowserError) -> BrowserCapability {
    let mut capability = BrowserRuntime::initial_capability();
    capability.status = if error.code == BrowserErrorCode::BrowserUnsupportedRuntime {
        BrowserRuntimeStatus::Unsupported
    } else {
        BrowserRuntimeStatus::Missing
    };
    capability.supported = capability.status != BrowserRuntimeStatus::Unsupported;
    capability.reason = Some(format!("{:?}", error.code));
    capability
}

fn runtime_state_mismatch() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser runtime state is inconsistent",
    )
}

fn incomplete_tab_cleanup_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserShuttingDown,
        "The previous browser tabs are still being cleaned up",
    )
    .retryable(true)
}
