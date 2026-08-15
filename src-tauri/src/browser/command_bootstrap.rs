use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::command_output::unavailable_error;
use super::command_runner::AgentBrowserCli;
use super::error::BrowserError;
#[cfg(target_os = "windows")]
use super::process::capture_process;
#[cfg(target_os = "windows")]
use super::windows_process::{launch_mode, spawn_unelevated, UnelevatedLaunchMode};

pub(super) async fn bootstrap(
    cli: &AgentBrowserCli,
    session: &str,
    args: &[&str],
    timeout: Duration,
    cancellation: CancellationToken,
) -> Result<(), BrowserError> {
    #[cfg(target_os = "windows")]
    match launch_mode() {
        Ok(UnelevatedLaunchMode::Required) => {
            return run_unelevated(cli, session, args, timeout, cancellation).await;
        }
        Ok(UnelevatedLaunchMode::Standard) => {}
        Err(error) => return Err(log_error(session, "detect", error)),
    }
    cli.run(session, args, timeout, cancellation)
        .await
        .map(|_| ())
}

#[cfg(target_os = "windows")]
async fn run_unelevated(
    cli: &AgentBrowserCli,
    session: &str,
    args: &[&str],
    timeout: Duration,
    cancellation: CancellationToken,
) -> Result<(), BrowserError> {
    let process = spawn_unelevated(
        cli.executable_path(),
        &cli.arguments(session, args),
        &cli.environment(),
    )
    .map_err(|error| log_error(session, "spawn", error))?;
    let record = capture_process(process.pid(), "agent-browser-bootstrap");
    let code = process
        .wait(timeout, cancellation, record.as_ref())
        .await
        .map_err(|error| log_error(session, "wait", error))?;
    finish(session, code)
}

#[cfg(target_os = "windows")]
fn finish(session: &str, exit_code: u32) -> Result<(), BrowserError> {
    if exit_code == 0 {
        tracing::info!(
            target: "iyw_claw_browser",
            session,
            "browser controller bootstrapped with the standard user token"
        );
        return Ok(());
    }
    tracing::error!(
        target: "iyw_claw_browser",
        session,
        exit_code,
        "standard-user browser controller bootstrap failed"
    );
    Err(unavailable_error())
}

#[cfg(target_os = "windows")]
fn log_error(session: &str, stage: &str, error: std::io::Error) -> BrowserError {
    tracing::error!(
        target: "iyw_claw_browser",
        session,
        stage,
        error = %error,
        "standard-user browser controller bootstrap failed"
    );
    unavailable_error()
}
