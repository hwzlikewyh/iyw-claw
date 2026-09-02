use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use tokio::sync::{watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::error::{BrowserError, BrowserErrorCode};
use crate::acp::version_center::{install_managed_tool, managed_browser_engine_executable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefetchState {
    Idle,
    Running,
    Ready,
    Failed,
}

#[derive(Debug, Clone)]
struct PrefetchFailure {
    occurred_at: Instant,
    error: BrowserError,
}

const FAILURE_RETRY_DELAY: Duration = Duration::from_secs(5);
const MISSING_ENGINE_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub(super) struct BrowserEnginePrefetch {
    state: Arc<Mutex<PrefetchState>>,
    state_change: watch::Sender<PrefetchState>,
    last_failure: Arc<Mutex<Option<PrefetchFailure>>>,
    database: Arc<RwLock<Option<DatabaseConnection>>>,
    data_root: PathBuf,
}

impl BrowserEnginePrefetch {
    pub(super) fn new(data_root: PathBuf) -> Self {
        let (state_change, _) = watch::channel(PrefetchState::Idle);
        Self {
            state: Arc::new(Mutex::new(PrefetchState::Idle)),
            state_change,
            last_failure: Arc::new(Mutex::new(None)),
            database: Arc::new(RwLock::new(None)),
            data_root,
        }
    }

    pub(super) async fn set_database(&self, database: DatabaseConnection) {
        *self.database.write().await = Some(database);
    }

    pub(super) fn schedule(&self, shutdown: CancellationToken) {
        let coordinator = self.clone();
        tokio::spawn(async move {
            if let Err(error) = coordinator.ensure_ready(shutdown).await {
                tracing::info!(
                    target: "iyw_claw_browser",
                    error_code = ?error.code,
                    "background browser engine prefetch deferred"
                );
            }
        });
    }

    pub(super) async fn ensure_ready(
        &self,
        cancellation: CancellationToken,
    ) -> Result<PathBuf, BrowserError> {
        if let Some(path) = managed_browser_engine_executable(&self.data_root).await {
            self.mark_ready().await;
            return Ok(path);
        }
        let mut state_change = self.state_change.subscribe();
        loop {
            let current_state = *self.state.lock().await;
            if current_state == PrefetchState::Ready {
                if let Some(path) = managed_browser_engine_executable(&self.data_root).await {
                    return Ok(path);
                }
            }
            if current_state == PrefetchState::Failed {
                if let Some(error) = self.cached_failure().await {
                    return Err(error);
                }
            }
            let should_install = {
                let mut state = self.state.lock().await;
                if *state == PrefetchState::Running {
                    false
                } else {
                    *state = PrefetchState::Running;
                    self.state_change.send_replace(PrefetchState::Running);
                    true
                }
            };
            if should_install {
                let result = self.install(cancellation.clone()).await;
                if let Err(error) = &result {
                    *self.last_failure.lock().await = Some(PrefetchFailure {
                        occurred_at: Instant::now(),
                        error: error.clone(),
                    });
                } else {
                    *self.last_failure.lock().await = None;
                }
                let state = if result.is_ok() {
                    PrefetchState::Ready
                } else {
                    PrefetchState::Failed
                };
                *self.state.lock().await = state;
                self.state_change.send_replace(state);
                return result;
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Err(BrowserError::shutting_down()),
                changed = state_change.changed() => {
                    if changed.is_err() {
                        return Err(engine_unavailable());
                    }
                }
            }
            if let Some(path) = managed_browser_engine_executable(&self.data_root).await {
                return Ok(path);
            }
            if matches!(*self.state.lock().await, PrefetchState::Failed) {
                return Err(self
                    .last_failure
                    .lock()
                    .await
                    .as_ref()
                    .map(|failure| failure.error.clone())
                    .unwrap_or_else(engine_unavailable));
            }
        }
    }

    async fn install(&self, cancellation: CancellationToken) -> Result<PathBuf, BrowserError> {
        if cancellation.is_cancelled() {
            return Err(BrowserError::shutting_down());
        }
        let Some(conn) = self.database.read().await.clone() else {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserRuntimeUnavailable,
                "The browser engine is waiting for desktop initialization",
            )
            .retryable(true));
        };
        let channel = crate::update::preferences::load(&conn)
            .await
            .map(|prefs| prefs.channel.as_str().to_string())
            .unwrap_or_else(|error| {
                tracing::info!(
                    target: "iyw_claw_browser",
                    error = %error,
                    "browser engine prefetch could not read update channel; using stable"
                );
                "stable".to_string()
            });
        let managed_result = install_managed_tool(
            &conn,
            &self.data_root,
            "browser-engine",
            None,
            &channel,
            false,
            None,
            None,
        )
        .await;
        if let Err(error) = managed_result {
            #[cfg(target_os = "windows")]
            {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    error_code = ?error.code,
                    "managed browser engine unavailable; trying verified local Chromium fallback"
                );
                return super::engine_download::ensure_managed_engine(
                    &self.data_root,
                    cancellation,
                )
                .await
                .map(|engine| engine.path);
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(engine_unavailable());
            }
        }
        if let Some(path) = managed_browser_engine_executable(&self.data_root).await {
            return Ok(path);
        }
        #[cfg(target_os = "windows")]
        {
            return super::engine_download::ensure_managed_engine(&self.data_root, cancellation)
                .await
                .map(|engine| engine.path);
        }
        #[cfg(not(target_os = "windows"))]
        Err(engine_unavailable())
    }

    async fn mark_ready(&self) {
        *self.state.lock().await = PrefetchState::Ready;
        self.state_change.send_replace(PrefetchState::Ready);
    }

    async fn cached_failure(&self) -> Option<BrowserError> {
        let failure = self.last_failure.lock().await.clone()?;
        let retry_delay = match failure.error.code {
            BrowserErrorCode::BrowserEngineNotFound => MISSING_ENGINE_RETRY_DELAY,
            _ => FAILURE_RETRY_DELAY,
        };
        (failure.occurred_at.elapsed() < retry_delay).then_some(failure.error)
    }
}

fn engine_unavailable() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The managed browser engine is unavailable",
    )
    .retryable(true)
}
