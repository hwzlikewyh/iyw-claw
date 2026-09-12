use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use sacp::schema::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReleaseTerminalRequest, ReleaseTerminalResponse, TerminalExitStatus, TerminalOutputRequest,
    TerminalOutputResponse, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Mutex;

type TerminalMap = HashMap<String, Arc<TerminalInstance>>;
const DEFAULT_OUTPUT_BYTE_LIMIT: u64 = 1_000_000;
#[derive(Debug)]
pub enum TerminalRuntimeError {
    InvalidParams(String),
    Internal(String),
}

impl TerminalRuntimeError {
    pub fn into_rpc_error(self) -> sacp::Error {
        match self {
            Self::InvalidParams(message) => sacp::Error::invalid_params().data(message),
            Self::Internal(message) => sacp::util::internal_error(message),
        }
    }
}

#[path = "terminal_instance.rs"]
mod instance;
#[path = "terminal_process.rs"]
mod process;

use instance::{TerminalInstance, TerminalReaders};
use process::TerminalProcess;

pub struct TerminalRuntime {
    terminals: Mutex<TerminalMap>,
    active_count: Arc<AtomicUsize>,
    /// Base environment merged into every spawned terminal command before
    /// the agent's per-request `env` is applied. This is where the iyw-claw
    /// git credential helper (`GIT_CONFIG_*`) lives so an agent that runs
    /// `git push` via the ACP `terminal/create` tool inherits the same
    /// auth path the agent process itself does. Per-request env from the
    /// agent overrides on key collision so an agent can still scrub or
    /// override anything explicitly.
    base_env: BTreeMap<String, String>,
    /// Fallback working directory applied to spawned terminals when the
    /// agent's `terminal/create` request omits `cwd`. The connection layer
    /// sets this to the session's resolved working directory so terminals
    /// default to the folder the conversation runs in instead of iyw-claw's own
    /// process cwd (often "/" on desktop, the dev crate dir in development).
    /// `None` leaves the process cwd inherited (legacy behavior).
    default_cwd: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct TerminalOutputDelta {
    pub output: String,
    pub next_offset: u64,
    pub had_gap: bool,
    pub truncated: bool,
    pub exit_status: Option<TerminalExitStatus>,
}

impl TerminalRuntime {
    /// Construct a runtime where every spawned command starts with `base_env`
    /// applied, before the agent's per-request env overrides are layered on
    /// top. Use this to propagate process-level invariants like the git
    /// credential helper across `terminal/create` invocations.
    pub fn with_base_env(
        base_env: BTreeMap<String, String>,
        active_count: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            terminals: Mutex::new(HashMap::new()),
            active_count,
            base_env,
            default_cwd: None,
        }
    }

    /// Set the fallback working directory used when a `terminal/create` request
    /// does not specify its own `cwd`. Chainable after `with_base_env`.
    pub fn with_default_cwd(mut self, default_cwd: Option<PathBuf>) -> Self {
        self.default_cwd = default_cwd;
        self
    }

    /// Apply stdio, working directory, and environment to a freshly built
    /// terminal command. Shared by the direct-exec and shell-fallback spawn
    /// paths in `create_terminal` so both honor the same cwd precedence and
    /// env layering.
    fn configure_command(
        &self,
        command: &mut tokio::process::Command,
        request: &CreateTerminalRequest,
    ) {
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Working directory. An explicit `cwd` from the agent (validated
        // absolute in `create_terminal`) is honored as-is, so a non-existent
        // directory surfaces as a loud spawn failure rather than silently
        // running somewhere else. Only when the agent omits `cwd` do we fall
        // back to the connection's session working directory — agents like
        // CodeBuddy omit it, which would otherwise inherit iyw-claw's own process
        // cwd instead of the folder the conversation runs in. The fallback is
        // guarded on `is_dir` so a not-yet-created session dir never turns into
        // a spawn failure (mirrors the cwd guard in `build_agent`).
        if let Some(cwd) = request.cwd.as_deref() {
            command.current_dir(cwd);
        } else if let Some(default_cwd) = self.default_cwd.as_deref() {
            if default_cwd.is_dir() {
                command.current_dir(default_cwd);
            }
        }

        // Apply the runtime's base env first (e.g. `GIT_CONFIG_*` for the
        // iyw-claw credential helper), then layer the agent's request env on top
        // so agents can still override or scrub specific keys.
        for (key, value) in &self.base_env {
            command.env(key, value);
        }
        for env_var in &request.env {
            command.env(&env_var.name, &env_var.value);
        }
    }

    pub async fn create_terminal(
        &self,
        request: CreateTerminalRequest,
    ) -> Result<CreateTerminalResponse, TerminalRuntimeError> {
        if let Some(cwd) = request.cwd.as_ref() {
            if !cwd.is_absolute() {
                return Err(TerminalRuntimeError::InvalidParams(
                    "terminal/create requires an absolute cwd when provided".to_string(),
                ));
            }
        }

        if request.command.trim().is_empty() {
            return Err(TerminalRuntimeError::InvalidParams(
                "terminal/create requires a non-empty command".to_string(),
            ));
        }

        let output_byte_limit = request
            .output_byte_limit
            .unwrap_or(DEFAULT_OUTPUT_BYTE_LIMIT);
        if output_byte_limit == 0 {
            return Err(TerminalRuntimeError::InvalidParams(
                "terminal/create outputByteLimit must be greater than 0".to_string(),
            ));
        }

        // Spawn the command. Try a direct exec first so a real program — one
        // resolved on PATH, an absolute path, or a relative/space-containing
        // path reachable through the request's cwd and env — runs exactly as
        // before, in the real spawn context. Only if the OS cannot find the
        // program (`NotFound`) AND the request looks like a whole shell line
        // crammed into `command` (empty args + embedded whitespace, the shape
        // CodeBuddy sends, e.g. "pnpm build") do we retry through the platform
        // shell so its `&&`, pipes, `$VAR`, and globs evaluate. Deciding off a
        // real failed spawn — rather than a pre-spawn `which` guess that runs
        // in iyw-claw's own cwd/env — means we never reroute a command that would
        // otherwise have run.
        #[cfg(windows)]
        let mut direct = super::windows_shell::command(&request.command, &request.args)
            .map_err(TerminalRuntimeError::InvalidParams)?;
        #[cfg(not(windows))]
        let mut direct = {
            let mut command = crate::process::tokio_command(&request.command);
            command.args(&request.args);
            command
        };
        self.configure_command(&mut direct, &request);

        let spawned = TerminalProcess::spawn(&mut direct).await;

        let mut process = match spawned {
            Ok(child) => child,
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    && request.args.is_empty()
                    && request.command.contains(char::is_whitespace) =>
            {
                let mut shell = shell_wrapped_command(&request.command);
                self.configure_command(&mut shell, &request);
                TerminalProcess::spawn(&mut shell).await.map_err(|err| {
                    TerminalRuntimeError::Internal(format!(
                        "failed to spawn terminal command {}: {err}",
                        request.command
                    ))
                })?
            }
            Err(err) => {
                return Err(TerminalRuntimeError::Internal(format!(
                    "failed to spawn terminal command {}: {err}",
                    request.command
                )));
            }
        };

        let stdout = process.child.stdout.take();
        let stderr = process.child.stderr.take();

        let terminal_id = format!("term_{}", uuid::Uuid::new_v4().simple());
        let terminal = Arc::new(TerminalInstance::new(
            request.session_id.to_string(),
            output_byte_limit,
            Arc::clone(&self.active_count),
        ));

        let mut handles = TerminalReaders::default();
        if let Some(reader) = stdout {
            let terminal_ref = terminal.clone();
            handles.0.push(tokio::spawn(async move {
                read_stream(reader, terminal_ref).await;
            }));
        }

        if let Some(reader) = stderr {
            let terminal_ref = terminal.clone();
            handles.0.push(tokio::spawn(async move {
                read_stream(reader, terminal_ref).await;
            }));
        }

        self.terminals
            .lock()
            .await
            .insert(terminal_id.clone(), Arc::clone(&terminal));
        tokio::spawn(terminal.monitor(process, handles));

        Ok(CreateTerminalResponse::new(terminal_id))
    }

    pub async fn terminal_output(
        &self,
        request: TerminalOutputRequest,
    ) -> Result<TerminalOutputResponse, TerminalRuntimeError> {
        let terminal = self
            .find_terminal(
                &request.terminal_id.to_string(),
                &request.session_id.to_string(),
            )
            .await?;

        terminal.refresh_exit_status().await?;
        let snapshot = terminal.snapshot().await;

        Ok(
            TerminalOutputResponse::new(snapshot.output, snapshot.truncated)
                .exit_status(snapshot.exit_status),
        )
    }

    pub async fn terminal_output_delta(
        &self,
        session_id: &str,
        terminal_id: &str,
        from_offset: Option<u64>,
    ) -> Result<TerminalOutputDelta, TerminalRuntimeError> {
        let terminal = self.find_terminal(terminal_id, session_id).await?;
        terminal.refresh_exit_status().await?;
        // 只复制调用方尚未读取的后缀，不先克隆整段终端输出。
        let snapshot = terminal.snapshot.lock().await;

        let output_len = u64::try_from(snapshot.output.len()).unwrap_or(u64::MAX);
        let base_offset = snapshot.output_base_offset;
        let end_offset = base_offset.saturating_add(output_len);
        let requested_offset = from_offset.unwrap_or(base_offset);
        let had_gap = from_offset
            .map(|offset| offset < base_offset)
            .unwrap_or(false);
        let start_offset = requested_offset.clamp(base_offset, end_offset);
        let mut start_index = usize::try_from(start_offset.saturating_sub(base_offset)).unwrap_or(0);
        while !snapshot.output.is_char_boundary(start_index) {
            start_index = start_index.saturating_sub(1);
        }
        let output = snapshot.output[start_index..].to_string();

        Ok(TerminalOutputDelta {
            output,
            next_offset: end_offset,
            had_gap,
            truncated: snapshot.truncated,
            exit_status: snapshot.exit_status.clone(),
        })
    }

    pub(crate) async fn terminal_has_exited(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Result<bool, TerminalRuntimeError> {
        let terminal = self.find_terminal(terminal_id, session_id).await?;
        terminal.refresh_exit_status().await?;
        let exited = !terminal.is_active();
        Ok(exited)
    }

    pub async fn wait_for_terminal_exit(
        &self,
        request: WaitForTerminalExitRequest,
    ) -> Result<WaitForTerminalExitResponse, TerminalRuntimeError> {
        let terminal = self
            .find_terminal(
                &request.terminal_id.to_string(),
                &request.session_id.to_string(),
            )
            .await?;
        let exit_status = terminal.wait_for_exit().await?;
        Ok(WaitForTerminalExitResponse::new(exit_status))
    }

    pub async fn kill_terminal(
        &self,
        request: KillTerminalRequest,
    ) -> Result<KillTerminalResponse, TerminalRuntimeError> {
        let terminal = self
            .find_terminal(
                &request.terminal_id.to_string(),
                &request.session_id.to_string(),
            )
            .await?;
        terminal.kill_command().await?;
        Ok(KillTerminalResponse::new())
    }

    pub async fn release_terminal(
        &self,
        request: ReleaseTerminalRequest,
    ) -> Result<ReleaseTerminalResponse, TerminalRuntimeError> {
        let terminal_id = request.terminal_id.to_string();
        let session_id = request.session_id.to_string();
        let terminal = self.find_terminal(&terminal_id, &session_id).await?;
        // 先完成停止和输出收尾；失败时保留所有权，后续仍能查询和重试。
        terminal.kill_command().await?;
        self.terminals.lock().await.remove(&terminal_id);
        Ok(ReleaseTerminalResponse::new())
    }

    pub async fn release_all_for_session(&self, session_id: &str) {
        let owned: Vec<_> = {
            let terminals = self.terminals.lock().await;
            terminals
                .iter()
                .filter(|(_, term)| term.session_id == session_id)
                .map(|(id, terminal)| (id.clone(), Arc::clone(terminal)))
                .collect()
        };
        for (_, terminal) in &owned { terminal.request_stop(); }
        for (id, terminal) in owned {
            if let Err(err) = terminal.kill_command().await {
                tracing::error!("[ACP] Failed to release terminal during cleanup: {err:?}");
            } else {
                self.terminals.lock().await.remove(&id);
            }
        }
    }

    async fn find_terminal(
        &self,
        terminal_id: &str,
        session_id: &str,
    ) -> Result<Arc<TerminalInstance>, TerminalRuntimeError> {
        let terminal = {
            let terminals = self.terminals.lock().await;
            terminals.get(terminal_id).cloned()
        }
        .ok_or_else(|| {
            TerminalRuntimeError::InvalidParams(format!("terminal {terminal_id} not found"))
        })?;

        if terminal.session_id != session_id {
            return Err(TerminalRuntimeError::InvalidParams(format!(
                "terminal {terminal_id} does not belong to session {session_id}"
            )));
        }

        Ok(terminal)
    }
}

impl Drop for TerminalRuntime {
    fn drop(&mut self) {
        for terminal in self.terminals.get_mut().values() {
            terminal.request_stop();
        }
    }
}

async fn read_stream<R>(mut reader: R, terminal: Arc<TerminalInstance>)
where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 4096];
    let mut pending = Vec::<u8>::new();
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                if !pending.is_empty() {
                    let text = String::from_utf8_lossy(&pending).to_string();
                    terminal.append_output(&text).await;
                    pending.clear();
                }
                break;
            }
            Ok(size) => {
                pending.extend_from_slice(&buffer[..size]);
                let decoded = decode_available_utf8(&mut pending);
                if !decoded.is_empty() {
                    terminal.append_output(&decoded).await;
                }
            }
            Err(error) => {
                terminal.record_read_error(error).await;
                break;
            }
        }
    }
}

/// Wrap a full shell command line so it executes through the platform shell.
/// Used when an agent passes an entire command line in `command` with empty
/// `args` (see `create_terminal`); the shell preserves the `&&`, pipes,
/// `$VAR`, and globs the agent's line relies on. Reuses `tokio_command` so the
/// shell still inherits iyw-claw's UTF-8 env and Windows program normalization.
#[cfg(not(windows))]
fn shell_wrapped_command(line: &str) -> tokio::process::Command {
    let mut command = crate::process::tokio_command("/bin/sh");
    command.arg("-c").arg(line);
    command
}

#[cfg(windows)]
fn shell_wrapped_command(line: &str) -> tokio::process::Command {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    let mut command = crate::process::tokio_command(comspec);
    command.arg("/C").arg(line);
    command
}

fn map_exit_status(status: std::process::ExitStatus) -> TerminalExitStatus {
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&status).map(|s| s.to_string());
    #[cfg(not(unix))]
    let signal: Option<String> = None;

    let exit_code = status.code().and_then(|code| u32::try_from(code).ok());
    TerminalExitStatus::new()
        .exit_code(exit_code)
        .signal(signal)
}

fn enforce_output_limit(output: &mut String, limit: usize) -> usize {
    if output.len() <= limit {
        return 0;
    }

    let mut start = output.len().saturating_sub(limit);
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }

    output.drain(..start);
    start
}

fn decode_available_utf8(pending: &mut Vec<u8>) -> String {
    let mut output = String::new();
    let mut consumed = 0usize;
    let mut remaining = pending.as_slice();

    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                output.push_str(text);
                consumed = consumed.saturating_add(remaining.len());
                break;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    if let Ok(text) = std::str::from_utf8(&remaining[..valid_up_to]) {
                        output.push_str(text);
                    }
                    consumed = consumed.saturating_add(valid_up_to);
                    remaining = &remaining[valid_up_to..];
                }

                match err.error_len() {
                    Some(invalid_len) => {
                        output.push_str(&String::from_utf8_lossy(&remaining[..invalid_len]));
                        consumed = consumed.saturating_add(invalid_len);
                        remaining = &remaining[invalid_len..];
                    }
                    None => break, // keep partial UTF-8 sequence for next chunk
                }
            }
        }
    }

    if consumed > 0 {
        pending.drain(..consumed);
    }
    output
}
