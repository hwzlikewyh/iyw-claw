use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::records::{RuntimeStartDecision, RuntimeTicket};
use super::runtime::{BrowserRuntime, BrowserRuntimeContext};
use super::state::BrowserState;
use super::types::{BrowserCapability, BrowserRuntimeStatus, BrowserStateSnapshot};

const TOTAL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(12);

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
        self.ensure_runtime_running().await?;
        Ok(self.snapshot().await)
    }

    pub async fn stop_browser_runtime(&self) -> Result<BrowserStateSnapshot, BrowserError> {
        self.shutdown().await?;
        Ok(self.snapshot().await)
    }

    pub(super) async fn ensure_runtime_running(
        &self,
    ) -> Result<BrowserRuntimeContext, BrowserError> {
        let runtime = self.desktop_runtime()?;
        if self.state.read().await.runtime.status == BrowserRuntimeStatus::Running {
            return runtime.context().await.ok_or_else(runtime_state_mismatch);
        }
        let capability = runtime.verify().await?;
        self.set_capability(capability).await;
        match self.begin_runtime_start().await? {
            RuntimeStartDecision::AlreadyRunning => {
                runtime.context().await.ok_or_else(runtime_state_mismatch)
            }
            RuntimeStartDecision::Start(ticket) => {
                self.start_runtime_with_ticket(runtime, ticket).await
            }
        }
    }

    pub async fn shutdown(&self) -> Result<(), BrowserError> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        if !self.state.write().await.begin_runtime_stop() {
            return Ok(());
        }
        let shutdown = async {
            self.close_all_controls().await;
            self.stop_cdp_observer().await;
            let (_, tabs_result) = tokio::join!(self.streams.close_all(), self.shutdown_tabs());
            let runtime_result = runtime.stop().await;
            tabs_result.and(runtime_result)
        };
        let result = tokio::time::timeout(TOTAL_SHUTDOWN_TIMEOUT, shutdown)
            .await
            .unwrap_or_else(|_| {
                Err(BrowserError::new(
                    BrowserErrorCode::BrowserOperationTimeout,
                    "The browser did not stop within the global shutdown budget",
                ))
            });
        let failure_code = result
            .as_ref()
            .err()
            .map(|error| format!("{:?}", error.code));
        self.state.write().await.finish_runtime_stop(failure_code);
        result
    }

    pub(super) fn with_runtime(
        capability: BrowserCapability,
        runtime: Option<Arc<BrowserRuntime>>,
    ) -> Self {
        Self {
            state: Arc::new(tokio::sync::RwLock::new(BrowserState::new(capability))),
            controls: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            runtime,
            tabs: Arc::new(super::tabs::BrowserTabRegistry::default()),
            streams: Arc::new(super::stream::BrowserStreamRegistry::default()),
            observer: Arc::new(tokio::sync::Mutex::new(None)),
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
    ) -> Result<BrowserRuntimeContext, BrowserError> {
        match runtime.start(ticket.generation).await {
            Ok(context) => match self.start_cdp_observer(&context).await {
                Err(error) => {
                    let _ = runtime.stop().await;
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
            if accepted {
                manager.stop_cdp_observer().await;
                manager.streams.close_all().await;
                let _ = manager.shutdown_tabs().await;
                manager.close_all_controls().await;
                tracing::error!(
                    target: "iyw_claw_browser",
                    runtime_generation = exited_generation,
                    "browser controller exited unexpectedly"
                );
            }
            runtime.release_exited(exited_generation).await;
            if accepted {
                manager.schedule_recovery(runtime, exited_generation);
            }
        });
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
