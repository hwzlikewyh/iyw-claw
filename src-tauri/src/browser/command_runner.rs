use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use super::error::{BrowserError, BrowserErrorCode};
use super::process::{capture_process, configure_hidden_process, kill_tree_checked, ProcessRecord};

const MAX_COMMAND_OUTPUT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct AgentBrowserCli {
    executable: PathBuf,
    socket_dir: PathBuf,
    profile_path: PathBuf,
    engine_path: PathBuf,
    download_path: PathBuf,
    screenshot_path: PathBuf,
}

impl AgentBrowserCli {
    pub fn new(
        executable: PathBuf,
        socket_dir: PathBuf,
        profile_path: PathBuf,
        engine_path: PathBuf,
        download_path: PathBuf,
        screenshot_path: PathBuf,
    ) -> Self {
        Self {
            executable,
            socket_dir,
            profile_path,
            engine_path,
            download_path,
            screenshot_path,
        }
    }

    pub async fn run(
        &self,
        session: &str,
        args: &[&str],
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        if cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        let mut command = self.command(session, args);
        let child = command.spawn().map_err(|_| unavailable_error())?;
        let process = child
            .id()
            .and_then(|pid| capture_process(pid, "agent-browser-client"));
        let output = collect_output(child);
        tokio::pin!(output);
        let result = tokio::select! {
            result = &mut output => result,
            _ = cancellation.cancelled() => {
                kill_client(process.as_ref()).await;
                return Err(cancelled_error());
            }
            _ = tokio::time::sleep(timeout) => {
                kill_client(process.as_ref()).await;
                return Err(BrowserError::new(
                    BrowserErrorCode::BrowserOperationTimeout,
                    "The browser operation timed out",
                ).retryable(true));
            }
        }?;
        parse_output(result.success, &result.stdout, &result.stderr)
    }

    pub fn pid_path(&self, session: &str) -> PathBuf {
        self.socket_dir.join(format!("{session}.pid"))
    }

    pub fn target_path(&self, session: &str) -> PathBuf {
        self.socket_dir.join(format!("{session}.target"))
    }

    pub fn socket_dir(&self) -> &Path {
        &self.socket_dir
    }

    pub fn download_path(&self) -> &Path {
        &self.download_path
    }

    pub fn screenshot_path(&self) -> &Path {
        &self.screenshot_path
    }

    pub fn executable_path(&self) -> &Path {
        &self.executable
    }

    fn command(&self, session: &str, args: &[&str]) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .arg("--session")
            .arg(session)
            .arg("--json")
            .args(args)
            .env("AGENT_BROWSER_SOCKET_DIR", &self.socket_dir)
            .env("AGENT_BROWSER_IDLE_TIMEOUT_MS", "0")
            .env("AGENT_BROWSER_NO_AUTO_DIALOG", "1")
            .env("AGENT_BROWSER_CONTENT_BOUNDARIES", "1")
            .env("AGENT_BROWSER_STREAM_QUALITY", "60")
            .env("AGENT_BROWSER_STREAM_MAX_WIDTH", "1600")
            .env("AGENT_BROWSER_STREAM_MAX_HEIGHT", "1000")
            .env("AGENT_BROWSER_PROFILE", &self.profile_path)
            .env("AGENT_BROWSER_EXECUTABLE_PATH", &self.engine_path)
            .env("AGENT_BROWSER_DOWNLOAD_PATH", &self.download_path)
            .env("AGENT_BROWSER_SCREENSHOT_DIR", &self.screenshot_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        configure_hidden_process(&mut command);
        command
    }
}

struct CollectedOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

async fn collect_output(mut child: Child) -> Result<CollectedOutput, BrowserError> {
    let stdout = child.stdout.take().ok_or_else(unavailable_error)?;
    let stderr = child.stderr.take().ok_or_else(unavailable_error)?;
    let (status, stdout, stderr) =
        tokio::join!(child.wait(), read_bounded(stdout), read_bounded(stderr));
    Ok(CollectedOutput {
        success: status.map_err(|_| unavailable_error())?.success(),
        stdout: stdout?,
        stderr: stderr?,
    })
}

async fn read_bounded(mut reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, BrowserError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|_| unavailable_error())?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > MAX_COMMAND_OUTPUT {
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserInternal,
                "The browser controller returned too much data",
            ));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

async fn kill_client(process: Option<&ProcessRecord>) {
    if let Some(process) = process {
        let _ = kill_tree_checked(process).await;
    }
}

fn parse_output(success: bool, stdout: &[u8], stderr: &[u8]) -> Result<Value, BrowserError> {
    if stdout.len() > MAX_COMMAND_OUTPUT || stderr.len() > MAX_COMMAND_OUTPUT {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserInternal,
            "The browser controller returned too much data",
        ));
    }
    let value: Value = serde_json::from_slice(stdout).map_err(|_| unavailable_error())?;
    if success && value.get("success").and_then(Value::as_bool) != Some(false) {
        return Ok(value);
    }
    let code = value
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Err(map_cli_error(code))
}

fn map_cli_error(code: &str) -> BrowserError {
    let mapped = match code {
        "tab_gone" => BrowserErrorCode::BrowserTabGone,
        "dialog_pending" => BrowserErrorCode::BrowserDialogPending,
        _ => BrowserErrorCode::BrowserRuntimeUnavailable,
    };
    BrowserError::new(mapped, "The browser controller rejected the operation").retryable(true)
}

fn unavailable_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser controller is unavailable",
    )
    .retryable(true)
}

fn cancelled_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserCancelled,
        "The browser operation was cancelled",
    )
}
