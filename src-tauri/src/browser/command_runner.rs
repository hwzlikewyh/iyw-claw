use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::command_bootstrap;
use super::command_diagnostics::{
    annotate_timeout, may_change_page, operation_name, timeout_error,
};
use super::command_output::{
    cancelled_error, collect_output, parse_output, unavailable_error, CollectedOutput,
};
use super::error::BrowserError;
use super::process::{
    capture_process, configure_hidden_process, find_processes_by_executable_arg, kill_tree_checked,
    ProcessRecord,
};

#[derive(Debug, Clone)]
pub(super) struct AgentBrowserCli {
    pub(super) executable: PathBuf,
    pub(super) socket_dir: PathBuf,
    pub(super) profile_path: PathBuf,
    pub(super) engine_path: PathBuf,
    pub(super) download_path: PathBuf,
    pub(super) screenshot_path: PathBuf,
    bootstrap_extensions: Vec<PathBuf>,
    bootstrap_browser_args: Vec<String>,
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
            bootstrap_extensions: Vec::new(),
            bootstrap_browser_args: Vec::new(),
        }
    }

    pub(super) fn with_bootstrap_extension(mut self, extension: PathBuf) -> Self {
        self.bootstrap_extensions.push(extension);
        self.bootstrap_browser_args
            .push("--headless=new".to_string());
        self
    }

    pub(super) fn with_browser_args(mut self, args: impl Into<String>) -> Self {
        self.bootstrap_browser_args.push(args.into());
        self
    }

    pub async fn run(
        &self,
        session: &str,
        args: &[&str],
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        self.run_with_mode(session, args, timeout, cancellation, false)
            .await
    }

    pub async fn bootstrap(
        &self,
        session: &str,
        args: &[&str],
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<(), BrowserError> {
        command_bootstrap::bootstrap(self, session, args, timeout, cancellation).await
    }

    pub(super) async fn run_bootstrap(
        &self,
        session: &str,
        args: &[&str],
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        self.run_with_mode(session, args, timeout, cancellation, true)
            .await
    }

    pub async fn run_pinned(
        &self,
        session: &str,
        cdp_url: &str,
        args: &[&str],
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<Value, BrowserError> {
        let mut pinned_args = Vec::with_capacity(args.len() + 3);
        pinned_args.extend_from_slice(&["--cdp", cdp_url, "--pin-tab"]);
        pinned_args.extend_from_slice(args);
        self.run(session, &pinned_args, timeout, cancellation).await
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

    pub async fn kill_profile_processes(&self) -> Result<(), BrowserError> {
        let profile = self.profile_path.to_string_lossy();
        kill_matching_processes(&self.engine_path, &profile, "browser-engine").await
    }

    pub async fn kill_sidecar_processes(&self) -> Result<(), BrowserError> {
        kill_matching_processes(&self.executable, "iyw-", "agent-browser-daemon").await
    }

    async fn run_with_mode(
        &self,
        session: &str,
        args: &[&str],
        timeout: Duration,
        cancellation: CancellationToken,
        bootstrap: bool,
    ) -> Result<Value, BrowserError> {
        if cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        let mut command = self.command(session, args, bootstrap);
        let child = command.spawn().map_err(|_| unavailable_error())?;
        let process = child
            .id()
            .and_then(|pid| capture_process(pid, "agent-browser-client"));
        let started = std::time::Instant::now();
        let operation = operation_name(args);
        log_command_started(session, operation, process.as_ref());
        let output = collect_output(child);
        tokio::pin!(output);
        let result = tokio::select! {
            result = &mut output => result,
            _ = cancellation.cancelled() => {
                kill_client(process.as_ref()).await;
                log_command_interrupted(session, operation, started, "cancelled");
                return Err(cancelled_error().effect_may_have_occurred(may_change_page(args)));
            }
            _ = tokio::time::sleep(timeout) => {
                kill_client(process.as_ref()).await;
                log_command_interrupted(session, operation, started, "timed_out");
                return Err(timeout_error(args, timeout));
            }
        }?;
        log_command_completed(session, operation, started, &result);
        parse_output(
            result.success,
            &result.stdout,
            &result.stderr,
            session,
            operation,
        )
        .map_err(|error| annotate_timeout(error, args))
    }

    fn command(&self, session: &str, args: &[&str], bootstrap: bool) -> Command {
        let mut command = Command::new(&self.executable);
        command.args(if bootstrap {
            self.bootstrap_arguments(session, args)
        } else {
            self.arguments(session, args)
        });
        let environment = if bootstrap {
            self.bootstrap_environment()
        } else {
            self.environment()
        };
        for (key, value) in environment {
            command.env(key, value);
        }
        configure_hidden_process(&mut command);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    pub(super) fn arguments(&self, session: &str, args: &[&str]) -> Vec<OsString> {
        let mut values = vec![
            OsString::from("--session"),
            OsString::from(session),
            OsString::from("--json"),
        ];
        values.extend(args.iter().map(|value| OsString::from(*value)));
        values
    }

    pub(super) fn bootstrap_arguments(&self, session: &str, args: &[&str]) -> Vec<OsString> {
        let mut values = self.arguments(session, args);
        for extension in &self.bootstrap_extensions {
            values.push(OsString::from("--extension"));
            values.push(extension.as_os_str().to_os_string());
        }
        values
    }

    pub(super) fn environment(&self) -> Vec<(OsString, OsString)> {
        let environment = vec![
            env("AGENT_BROWSER_SOCKET_DIR", self.socket_dir.as_os_str()),
            env("AGENT_BROWSER_IDLE_TIMEOUT_MS", "0"),
            env("AGENT_BROWSER_NO_AUTO_DIALOG", "1"),
            env("AGENT_BROWSER_CONTENT_BOUNDARIES", "1"),
            env("AGENT_BROWSER_STREAM_QUALITY", "85"),
            env("AGENT_BROWSER_STREAM_MAX_WIDTH", "4096"),
            env("AGENT_BROWSER_STREAM_MAX_HEIGHT", "2560"),
            env("AGENT_BROWSER_PROFILE", self.profile_path.as_os_str()),
            env(
                "AGENT_BROWSER_EXECUTABLE_PATH",
                self.engine_path.as_os_str(),
            ),
            env(
                "AGENT_BROWSER_DOWNLOAD_PATH",
                self.download_path.as_os_str(),
            ),
            env(
                "AGENT_BROWSER_SCREENSHOT_DIR",
                self.screenshot_path.as_os_str(),
            ),
        ];
        environment
    }

    pub(super) fn bootstrap_environment(&self) -> Vec<(OsString, OsString)> {
        let mut environment = self.environment();
        if !self.bootstrap_browser_args.is_empty() {
            environment.push(env(
                "AGENT_BROWSER_ARGS",
                self.bootstrap_browser_args.join("\n"),
            ));
        }
        environment
    }
}

async fn kill_matching_processes(
    executable: &Path,
    argument_fragment: &str,
    label: &str,
) -> Result<(), BrowserError> {
    let mut processes = find_processes_by_executable_arg(executable, argument_fragment, label);
    processes.sort_by_key(|process| (process.pid, process.started_at));
    processes.dedup_by_key(|process| (process.pid, process.started_at));
    let results = futures_util::future::join_all(processes.iter().map(kill_tree_checked)).await;
    results
        .into_iter()
        .find_map(Result::err)
        .map_or(Ok(()), Err)
}

fn env(key: impl Into<OsString>, value: impl Into<OsString>) -> (OsString, OsString) {
    (key.into(), value.into())
}

fn log_command_started(session: &str, operation: &str, process: Option<&ProcessRecord>) {
    tracing::info!(
        target: "iyw_claw_browser",
        session,
        operation,
        client_pid = process.map(|record| record.pid),
        "browser controller command started"
    );
}

fn log_command_interrupted(
    session: &str,
    operation: &str,
    started: std::time::Instant,
    outcome: &str,
) {
    tracing::warn!(
        target: "iyw_claw_browser",
        session,
        operation,
        outcome,
        duration_ms = started.elapsed().as_millis() as u64,
        "browser controller command interrupted"
    );
}

fn log_command_completed(
    session: &str,
    operation: &str,
    started: std::time::Instant,
    result: &CollectedOutput,
) {
    tracing::info!(
        target: "iyw_claw_browser",
        session,
        operation,
        duration_ms = started.elapsed().as_millis() as u64,
        success = result.success,
        exit_code = result.exit_code,
        stdout_bytes = result.stdout.len(),
        stderr_bytes = result.stderr.len(),
        "browser controller command completed"
    );
}

async fn kill_client(process: Option<&ProcessRecord>) {
    if let Some(process) = process {
        let _ = kill_tree_checked(process).await;
    }
}
