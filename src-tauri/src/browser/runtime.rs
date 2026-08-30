use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::engine::BrowserEngine;
use super::error::{BrowserError, BrowserErrorCode};
use super::process::ProcessRecord;
use super::profile::ProfileGuard;
use super::runtime_launch;
use super::sidecar::AGENT_BROWSER_VERSION;
use super::types::{BrowserCapability, BrowserRuntimeStatus};

const PROCESS_WATCH_INTERVAL: Duration = Duration::from_millis(250);

#[path = "runtime_dependencies.rs"]
mod dependencies;
mod shutdown;

#[derive(Debug)]
pub(super) struct BrowserRuntime {
    data_root: PathBuf,
    verified: Mutex<Option<VerifiedDependencies>>,
    current: Mutex<Option<RuntimeHandle>>,
    pending_cleanup: Mutex<Option<RuntimeCleanupHandle>>,
    mutation: Mutex<()>,
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedDependencies {
    pub sidecar: PathBuf,
    pub engine: BrowserEngine,
}

#[derive(Debug)]
pub(super) struct RuntimeHandle {
    pub id: String,
    pub generation: u64,
    pub controller_session: String,
    pub cli: AgentBrowserCli,
    pub cdp_url: String,
    pub daemon: ProcessRecord,
    pub runtime_dir: PathBuf,
    pub watcher_cancel: CancellationToken,
    pub _profile: ProfileGuard,
}

#[derive(Debug)]
pub(super) struct RuntimeCleanupHandle {
    pub id: String,
    pub generation: u64,
    pub controller_session: String,
    pub cli: AgentBrowserCli,
    pub daemon: Option<ProcessRecord>,
    pub runtime_dir: PathBuf,
    pub profile: ProfileGuard,
}

#[derive(Debug, Clone)]
pub(super) struct BrowserRuntimeContext {
    pub runtime_id: String,
    pub generation: u64,
    pub controller_session: String,
    pub cli: AgentBrowserCli,
    pub cdp_url: String,
    pub download_path: PathBuf,
}

pub(super) struct RuntimeExitWatch {
    generation: u64,
    daemon: ProcessRecord,
    cancellation: CancellationToken,
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedBrowserProcessSnapshot {
    pub pid: u32,
    pub started_at: u64,
    pub executable: PathBuf,
}

impl BrowserRuntime {
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            data_root,
            verified: Mutex::new(None),
            current: Mutex::new(None),
            pending_cleanup: Mutex::new(None),
            mutation: Mutex::new(()),
        }
    }

    pub fn initial_capability() -> BrowserCapability {
        BrowserCapability {
            supported: cfg!(all(target_os = "windows", target_arch = "x86_64")),
            status: if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
                BrowserRuntimeStatus::Verifying
            } else {
                BrowserRuntimeStatus::Unsupported
            },
            reason: None,
            platform: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            sidecar_version: AGENT_BROWSER_VERSION.to_string(),
            sidecar_verified: false,
            engine: None,
        }
    }

    pub async fn verify(&self) -> Result<BrowserCapability, BrowserError> {
        if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserUnsupportedRuntime,
                "The shared browser currently requires Windows x64 desktop",
            ));
        }
        if let Some(dependencies) = self.verified.lock().await.clone() {
            return Ok(dependencies.capability());
        }
        let started = std::time::Instant::now();
        let dependencies = self.resolve_dependencies().await?;
        let capability = dependencies.capability();
        tracing::info!(
            target: "iyw_claw_browser",
            duration_ms = started.elapsed().as_millis() as u64,
            "browser capability verification completed"
        );
        Ok(capability)
    }

    pub async fn start(
        &self,
        generation: u64,
        cancellation: CancellationToken,
    ) -> Result<BrowserRuntimeContext, BrowserError> {
        let _mutation = self.mutation.lock().await;
        if let Some(mut cleanup) = self.pending_cleanup.lock().await.take() {
            if let Err(error) = runtime_launch::cleanup_partial_owner(&mut cleanup).await {
                *self.pending_cleanup.lock().await = Some(cleanup);
                return Err(error);
            }
        }
        if let Some(context) = self.context().await {
            return (context.generation == generation)
                .then_some(context)
                .ok_or_else(incomplete_cleanup_error);
        }
        let dependencies = self.prepare_dependencies(cancellation.clone()).await?;
        let handle =
            match runtime_launch::launch(&self.data_root, dependencies, generation, cancellation)
                .await
            {
                Ok(handle) => handle,
                Err(failure) => {
                    if let Some(cleanup) = failure.cleanup {
                        *self.pending_cleanup.lock().await = Some(cleanup);
                    }
                    return Err(failure.error);
                }
            };
        let context = handle.context();
        *self.current.lock().await = Some(handle);
        tracing::info!(
            target: "iyw_claw_browser",
            runtime_generation = generation,
            daemon_pid = context.cli.pid_path(&context.controller_session).display().to_string(),
            "browser runtime started"
        );
        Ok(context)
    }

    pub async fn context(&self) -> Option<BrowserRuntimeContext> {
        self.current
            .lock()
            .await
            .as_ref()
            .map(RuntimeHandle::context)
    }

    pub async fn take_exit_watch(&self, generation: u64) -> Option<RuntimeExitWatch> {
        let current = self.current.lock().await;
        let handle = current
            .as_ref()
            .filter(|handle| handle.generation == generation)?;
        Some(RuntimeExitWatch {
            generation,
            daemon: handle.daemon.clone(),
            cancellation: handle.watcher_cancel.clone(),
        })
    }

    pub async fn process_snapshot(&self) -> Option<ManagedBrowserProcessSnapshot> {
        let current = self.current.lock().await;
        let daemon = &current.as_ref()?.daemon;
        Some(ManagedBrowserProcessSnapshot {
            pid: daemon.pid,
            started_at: daemon.started_at,
            executable: daemon.executable.clone()?,
        })
    }

    pub async fn reclaim_stale_profile(&self) -> Result<usize, BrowserError> {
        let dependencies = self.dependencies().await?;
        ProfileGuard::reclaim_stale(
            &self.data_root.join("browser"),
            &dependencies.sidecar,
            &dependencies.engine.path,
        )
        .await
    }
}

impl VerifiedDependencies {
    fn capability(&self) -> BrowserCapability {
        BrowserCapability {
            supported: true,
            status: BrowserRuntimeStatus::Ready,
            reason: None,
            platform: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            sidecar_version: AGENT_BROWSER_VERSION.to_string(),
            sidecar_verified: true,
            engine: Some(self.engine.summary()),
        }
    }
}

impl RuntimeHandle {
    fn context(&self) -> BrowserRuntimeContext {
        BrowserRuntimeContext {
            runtime_id: self.id.clone(),
            generation: self.generation,
            controller_session: self.controller_session.clone(),
            cli: self.cli.clone(),
            cdp_url: self.cdp_url.clone(),
            download_path: self.cli.download_path().to_path_buf(),
        }
    }
}

impl RuntimeExitWatch {
    pub async fn wait(self) -> Option<u64> {
        loop {
            if !super::process::process_matches(&self.daemon) {
                return Some(self.generation);
            }
            tokio::select! {
                _ = self.cancellation.cancelled() => return None,
                _ = tokio::time::sleep(PROCESS_WATCH_INTERVAL) => {}
            }
        }
    }
}

pub(super) fn unavailable_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime could not be started",
    )
    .retryable(true)
}

fn incomplete_cleanup_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserShuttingDown,
        "The previous browser runtime is still being cleaned up",
    )
    .retryable(true)
}
