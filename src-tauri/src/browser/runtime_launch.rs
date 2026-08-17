use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Url;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::command_runner::AgentBrowserCli;
use super::error::BrowserError;
use super::process::{kill_tree_checked, wait_for_pid_file, ProcessRecord};
use super::profile::ProfileGuard;
use super::runtime::{
    unavailable_error, RuntimeCleanupHandle, RuntimeHandle, VerifiedDependencies,
};

const START_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct RuntimeLaunchFailure {
    pub error: BrowserError,
    pub cleanup: Option<RuntimeCleanupHandle>,
}

pub(super) async fn launch(
    data_root: &Path,
    dependencies: VerifiedDependencies,
    generation: u64,
    cancellation: CancellationToken,
) -> Result<RuntimeHandle, RuntimeLaunchFailure> {
    if cancellation.is_cancelled() {
        return Err(launch_failure(BrowserError::shutting_down(), None));
    }
    let runtime_id = Uuid::new_v4().simple().to_string();
    let controller_session = format!("iyw-runtime-{}", &runtime_id[..12]);
    let runtime_dir = data_root
        .join("browser")
        .join(format!("runtime-{runtime_id}"));
    let socket_dir = runtime_dir.join("sockets");
    create_dir(&socket_dir)
        .await
        .map_err(|error| launch_failure(error, None))?;
    let profile = acquire_profile(data_root, &runtime_id, &runtime_dir, &dependencies)
        .await
        .map_err(|error| launch_failure(error, None))?;
    let download_path = prepare_download_path(data_root, &runtime_dir)
        .await
        .map_err(|error| launch_failure(error, None))?;
    let screenshot_path = prepare_screenshot_path(&runtime_dir)
        .await
        .map_err(|error| launch_failure(error, None))?;
    let cli = AgentBrowserCli::new(
        dependencies.sidecar,
        socket_dir,
        profile.profile_path.clone(),
        dependencies.engine.path,
        download_path,
        screenshot_path,
    );
    let mut cleanup = RuntimeCleanupHandle {
        id: runtime_id,
        generation,
        controller_session,
        cli,
        daemon: None,
        runtime_dir,
        profile,
    };
    let cdp_url = match launch_controller(&mut cleanup, cancellation.clone()).await {
        Ok(cdp_url) => cdp_url,
        Err(error) => return Err(rollback_launch(cleanup, error).await),
    };
    if cancellation.is_cancelled() {
        return Err(rollback_launch(cleanup, BrowserError::shutting_down()).await);
    }
    let Some(daemon) = cleanup.daemon.take() else {
        return Err(rollback_launch(cleanup, unavailable_error()).await);
    };
    Ok(RuntimeHandle {
        id: cleanup.id,
        generation: cleanup.generation,
        controller_session: cleanup.controller_session,
        cli: cleanup.cli,
        cdp_url,
        daemon,
        runtime_dir: cleanup.runtime_dir,
        watcher_cancel: CancellationToken::new(),
        _profile: cleanup.profile,
    })
}

async fn prepare_screenshot_path(runtime_dir: &Path) -> Result<PathBuf, BrowserError> {
    let path = runtime_dir.join("screenshots");
    if let Err(error) = create_dir(&path).await {
        remove_runtime_dir(runtime_dir).await;
        return Err(error);
    }
    Ok(path)
}

async fn acquire_profile(
    data_root: &Path,
    runtime_id: &str,
    runtime_dir: &Path,
    dependencies: &VerifiedDependencies,
) -> Result<ProfileGuard, BrowserError> {
    match ProfileGuard::acquire(
        &data_root.join("browser"),
        runtime_id,
        &dependencies.sidecar,
        &dependencies.engine.path,
    )
    .await
    {
        Ok(profile) => Ok(profile),
        Err(error) => {
            remove_runtime_dir(runtime_dir).await;
            Err(error)
        }
    }
}

async fn prepare_download_path(
    data_root: &Path,
    runtime_dir: &Path,
) -> Result<PathBuf, BrowserError> {
    let path = dirs::download_dir()
        .unwrap_or_else(|| data_root.join("browser/downloads"))
        .join("原助理");
    if let Err(error) = create_dir(&path).await {
        remove_runtime_dir(runtime_dir).await;
        return Err(error);
    }
    Ok(path)
}

async fn launch_controller(
    cleanup: &mut RuntimeCleanupHandle,
    cancellation: CancellationToken,
) -> Result<String, BrowserError> {
    cleanup
        .cli
        .bootstrap(
            &cleanup.controller_session,
            &["open", "about:blank"],
            START_TIMEOUT,
            cancellation.clone(),
        )
        .await?;
    let daemon = wait_for_pid_file(
        &cleanup.cli.pid_path(&cleanup.controller_session),
        cleanup.cli.executable_path(),
        Duration::from_secs(3),
    )
    .await?;
    cleanup.profile.bind_daemon(&daemon)?;
    cleanup.daemon = Some(daemon);
    let response = cleanup
        .cli
        .run(
            &cleanup.controller_session,
            &["get", "cdp-url"],
            COMMAND_TIMEOUT,
            cancellation,
        )
        .await?;
    parse_cdp_url(&response)
}

pub(super) async fn cleanup_partial_owner(
    cleanup: &mut RuntimeCleanupHandle,
) -> Result<(), BrowserError> {
    if cleanup.daemon.is_none() {
        cleanup.daemon = published_daemon(&cleanup.cli, &cleanup.controller_session).await;
    }
    let initial_daemon_result = match cleanup.daemon.as_ref() {
        Some(daemon) => kill_tree_checked(daemon).await,
        None => Ok(()),
    };
    if let Err(error) = initial_daemon_result {
        tracing::warn!(
            target: "iyw_claw_browser",
            error_code = ?error.code,
            "partial browser daemon required sweep retry"
        );
    }
    let (sidecar_result, engine_result) = tokio::join!(
        cleanup.cli.kill_sidecar_processes(),
        cleanup.cli.kill_profile_processes()
    );
    let daemon_result = match cleanup.daemon.as_ref() {
        Some(daemon) => kill_tree_checked(daemon).await,
        None => Ok(()),
    };
    daemon_result.and(sidecar_result).and(engine_result)?;
    remove_runtime_dir(&cleanup.runtime_dir).await;
    Ok(())
}

async fn rollback_launch(
    mut cleanup: RuntimeCleanupHandle,
    error: BrowserError,
) -> RuntimeLaunchFailure {
    if let Err(cleanup_error) = cleanup_partial_owner(&mut cleanup).await {
        tracing::error!(
            target: "iyw_claw_browser",
            error_code = ?cleanup_error.code,
            "partial browser startup cleanup remains incomplete"
        );
        return launch_failure(error, Some(cleanup));
    }
    launch_failure(error, None)
}

async fn published_daemon(cli: &AgentBrowserCli, session: &str) -> Option<ProcessRecord> {
    wait_for_pid_file(
        &cli.pid_path(session),
        cli.executable_path(),
        Duration::from_millis(200),
    )
    .await
    .ok()
}

fn launch_failure(
    error: BrowserError,
    cleanup: Option<RuntimeCleanupHandle>,
) -> RuntimeLaunchFailure {
    RuntimeLaunchFailure { error, cleanup }
}

fn parse_cdp_url(response: &Value) -> Result<String, BrowserError> {
    response_data(response)
        .get("cdpUrl")
        .and_then(Value::as_str)
        .filter(|url| is_loopback_cdp(url))
        .map(str::to_string)
        .ok_or_else(unavailable_error)
}

fn response_data(response: &Value) -> &Value {
    response.get("data").unwrap_or(response)
}

fn is_loopback_cdp(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "ws" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
    })
}

async fn create_dir(path: &Path) -> Result<(), BrowserError> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|_| unavailable_error())
}

async fn remove_runtime_dir(path: &Path) {
    let _ = tokio::fs::remove_dir_all(path).await;
}
