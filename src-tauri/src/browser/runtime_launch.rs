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
use super::runtime::{unavailable_error, RuntimeHandle, VerifiedDependencies};

const START_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) async fn launch(
    data_root: &Path,
    dependencies: VerifiedDependencies,
    generation: u64,
) -> Result<RuntimeHandle, BrowserError> {
    let runtime_id = Uuid::new_v4().simple().to_string();
    let controller_session = format!("iyw-runtime-{}", &runtime_id[..12]);
    let runtime_dir = data_root
        .join("browser")
        .join(format!("runtime-{runtime_id}"));
    let socket_dir = runtime_dir.join("sockets");
    create_dir(&socket_dir).await?;
    let profile = acquire_profile(data_root, &runtime_id, &runtime_dir, &dependencies).await?;
    let download_path = prepare_download_path(data_root, &runtime_dir).await?;
    let screenshot_path = prepare_screenshot_path(&runtime_dir).await?;
    let cli = AgentBrowserCli::new(
        dependencies.sidecar,
        socket_dir,
        profile.profile_path.clone(),
        dependencies.engine.path,
        download_path,
        screenshot_path,
    );
    let (cdp_url, daemon) =
        launch_or_rollback(&cli, &controller_session, &runtime_dir, &profile).await?;
    Ok(RuntimeHandle {
        id: runtime_id,
        generation,
        controller_session,
        cli,
        cdp_url,
        daemon,
        runtime_dir,
        watcher_cancel: CancellationToken::new(),
        _profile: profile,
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

async fn launch_or_rollback(
    cli: &AgentBrowserCli,
    session: &str,
    runtime_dir: &Path,
    profile: &ProfileGuard,
) -> Result<(String, ProcessRecord), BrowserError> {
    match launch_controller(cli, session, profile).await {
        Ok(result) => Ok(result),
        Err(error) => {
            cleanup_partial(cli, session, runtime_dir).await;
            Err(error)
        }
    }
}

async fn launch_controller(
    cli: &AgentBrowserCli,
    session: &str,
    profile: &ProfileGuard,
) -> Result<(String, ProcessRecord), BrowserError> {
    let cancellation = CancellationToken::new();
    cli.run(
        session,
        &["open", "about:blank"],
        START_TIMEOUT,
        cancellation.clone(),
    )
    .await?;
    let daemon = wait_for_pid_file(
        &cli.pid_path(session),
        cli.executable_path(),
        Duration::from_secs(3),
    )
    .await?;
    profile.bind_daemon(&daemon)?;
    let response = cli
        .run(session, &["get", "cdp-url"], COMMAND_TIMEOUT, cancellation)
        .await?;
    let cdp_url = parse_cdp_url(&response)?;
    Ok((cdp_url, daemon))
}

async fn cleanup_partial(cli: &AgentBrowserCli, session: &str, runtime_dir: &Path) {
    if tokio::fs::metadata(cli.pid_path(session)).await.is_ok() {
        graceful_close(cli, session).await;
        kill_published_daemon(cli, session).await;
    }
    remove_runtime_dir(runtime_dir).await;
}

async fn graceful_close(cli: &AgentBrowserCli, session: &str) {
    let _ = cli
        .run(
            session,
            &["close"],
            GRACEFUL_STOP_TIMEOUT,
            CancellationToken::new(),
        )
        .await;
}

async fn kill_published_daemon(cli: &AgentBrowserCli, session: &str) {
    let daemon = wait_for_pid_file(
        &cli.pid_path(session),
        cli.executable_path(),
        Duration::from_millis(200),
    )
    .await;
    if let Ok(daemon) = daemon {
        let _ = kill_tree_checked(&daemon).await;
    }
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
