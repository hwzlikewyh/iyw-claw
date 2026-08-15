use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::engine::{detect_engine, BrowserEngine};
use super::error::{BrowserError, BrowserErrorCode};
use super::process::{kill_tree_checked, wait_for_exit, ProcessRecord};
use super::profile::ProfileGuard;
use super::runtime_launch;
use super::sidecar::{self, AGENT_BROWSER_VERSION};
use super::types::{BrowserCapability, BrowserRuntimeStatus};

const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(4);
const PROCESS_WATCH_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub(super) struct BrowserRuntime {
    data_root: PathBuf,
    verified: Mutex<Option<VerifiedDependencies>>,
    current: Mutex<Option<RuntimeHandle>>,
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

impl BrowserRuntime {
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            data_root,
            verified: Mutex::new(None),
            current: Mutex::new(None),
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
        let mut verified = self.verified.lock().await;
        if let Some(dependencies) = verified.as_ref() {
            return Ok(dependencies.capability());
        }
        let started = std::time::Instant::now();
        let sidecar = sidecar::verify_sidecar().await?;
        let engine = detect_engine().await?;
        let dependencies = VerifiedDependencies { sidecar, engine };
        let capability = dependencies.capability();
        *verified = Some(dependencies);
        tracing::info!(
            target: "iyw_claw_browser",
            duration_ms = started.elapsed().as_millis() as u64,
            "browser capability verification completed"
        );
        Ok(capability)
    }

    pub async fn start(&self, generation: u64) -> Result<BrowserRuntimeContext, BrowserError> {
        let _mutation = self.mutation.lock().await;
        if let Some(context) = self.context().await {
            return Ok(context);
        }
        self.verify().await?;
        let dependencies = self.dependencies().await?;
        let handle = runtime_launch::launch(&self.data_root, dependencies, generation).await?;
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

    pub async fn stop(&self) -> Result<(), BrowserError> {
        let _mutation = self.mutation.lock().await;
        let Some(mut handle) = ({ self.current.lock().await.take() }) else {
            return Ok(());
        };
        handle.watcher_cancel.cancel();
        match tokio::time::timeout(SHUTDOWN_TIMEOUT, stop_handle(&mut handle)).await {
            Ok(result) => result,
            Err(_) => {
                let kill_result = kill_tree_checked(&handle.daemon).await;
                let engine_result = handle.cli.kill_profile_processes().await;
                let _ = tokio::fs::remove_dir_all(&handle.runtime_dir).await;
                kill_result?;
                engine_result?;
                Err(BrowserError::new(
                    BrowserErrorCode::BrowserOperationTimeout,
                    "The browser runtime did not stop within the shutdown budget",
                ))
            }
        }
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

    pub async fn release_exited(&self, generation: u64) {
        let _mutation = self.mutation.lock().await;
        let handle = {
            let mut current = self.current.lock().await;
            if current.as_ref().map(|handle| handle.generation) != Some(generation) {
                return;
            }
            current.take()
        };
        if let Some(handle) = handle {
            handle.watcher_cancel.cancel();
            if let Err(error) = handle.cli.kill_profile_processes().await {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    error_code = ?error.code,
                    "failed to clean browser engine after controller exit"
                );
            }
            let _ = tokio::fs::remove_dir_all(&handle.runtime_dir).await;
        }
    }

    async fn dependencies(&self) -> Result<VerifiedDependencies, BrowserError> {
        if let Some(dependencies) = self.verified.lock().await.clone() {
            return Ok(dependencies);
        }
        self.verify().await?;
        self.verified
            .lock()
            .await
            .clone()
            .ok_or_else(unavailable_error)
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

async fn stop_handle(handle: &mut RuntimeHandle) -> Result<(), BrowserError> {
    let result = handle
        .cli
        .run(
            &handle.controller_session,
            &["close"],
            GRACEFUL_STOP_TIMEOUT,
            CancellationToken::new(),
        )
        .await;
    if !wait_for_exit(&handle.daemon, GRACEFUL_STOP_TIMEOUT).await {
        kill_tree_checked(&handle.daemon).await?;
    }
    handle.cli.kill_profile_processes().await?;
    let _ = tokio::fs::remove_dir_all(&handle.runtime_dir).await;
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.code == BrowserErrorCode::BrowserRuntimeUnavailable => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn unavailable_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser runtime could not be started",
    )
    .retryable(true)
}
