use std::collections::HashMap;

use codex_exec_server_protocol::JSONRPCErrorError;
use codex_sandboxing::SandboxCommand;
use codex_sandboxing::SandboxExecRequest;
use codex_sandboxing::SandboxManager;
use codex_sandboxing::SandboxTransformRequest;
use codex_utils_path_uri::PathUri;
use codex_utils_pty::Child;
use codex_utils_pty::ChildStdin;
use codex_utils_pty::Command;
#[cfg(target_os = "macos")]
use codex_utils_pty::DescriptorPolicy;
use codex_utils_pty::SpawnFallback;
use tokio::io::AsyncWriteExt;

use crate::ExecServerRuntimeOptions;
use crate::FileSystemSandboxContext;
use crate::fs_helper::CODEX_FS_HELPER_ARG1;
use crate::fs_helper::FsHelperPayload;
use crate::fs_helper::FsHelperRequest;
use crate::fs_helper::FsHelperResponse;
use crate::rpc::internal_error;
use crate::rpc::invalid_request;

#[cfg(windows)]
#[path = "fs_sandbox_windows.rs"]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{drain_helper_stderr, read_helper_response, reap_helper_after_response};

const FS_HELPER_ENV_ALLOWLIST: &[&str] = &["PATH", "TMPDIR", "TMP", "TEMP"];
#[cfg(debug_assertions)]
const FS_HELPER_BAZEL_BWRAP_ENV_ALLOWLIST: &[&str] = &[
    "CARGO_BIN_EXE_bwrap",
    "RUNFILES_DIR",
    "RUNFILES_MANIFEST_FILE",
    "RUNFILES_MANIFEST_ONLY",
    "TEST_SRCDIR",
    "TEST_WORKSPACE",
];

#[derive(Clone, Debug)]
pub(crate) struct FileSystemSandboxRunner {
    runtime_paths: ExecServerRuntimeOptions,
    helper_env: HashMap<String, String>,
}

impl FileSystemSandboxRunner {
    pub(crate) fn new(runtime_paths: ExecServerRuntimeOptions) -> Self {
        Self {
            runtime_paths,
            helper_env: helper_env(),
        }
    }

    #[tracing::instrument(name = "fs.sandbox_request", skip_all)]
    pub(crate) async fn run(
        &self,
        sandbox: &FileSystemSandboxContext,
        request: FsHelperRequest,
    ) -> Result<FsHelperPayload, JSONRPCErrorError> {
        let command = self.prepare_command(sandbox).await?;
        let request_json = serde_json::to_vec(&request).map_err(json_error)?;
        run_command(command, request_json).await
    }

    pub(crate) async fn prepare_command(
        &self,
        sandbox: &FileSystemSandboxContext,
    ) -> Result<SandboxExecRequest, JSONRPCErrorError> {
        let command = self.sandbox_command(sandbox)?;
        crate::run_integrity_checks((&command).into()).await;
        Ok(command)
    }

    #[tracing::instrument(
        name = "fs.sandbox_prepare",
        skip_all,
        fields(permission_entries = tracing::field::Empty)
    )]
    fn sandbox_command(
        &self,
        sandbox: &FileSystemSandboxContext,
    ) -> Result<SandboxExecRequest, JSONRPCErrorError> {
        let helper = &self.runtime_paths.codex_self_exe;
        let mut helper_readable_paths = Vec::new();
        if !sandbox.use_legacy_landlock {
            helper_readable_paths.push(helper.clone());
            if let Some(linux_helper) = &self.runtime_paths.codex_linux_sandbox_exe
                && linux_helper != helper
            {
                helper_readable_paths.push(linux_helper.clone());
            }
        }

        let sandbox_manager = SandboxManager::for_file_system_helpers()
            .with_linux_sandbox_pid_namespace(self.runtime_paths.linux_sandbox_pid_namespace);
        #[cfg(target_os = "macos")]
        let sandbox_manager = sandbox_manager.with_allowed_symlinked_codex_home(
            self.runtime_paths.allowed_symlinked_codex_home.clone(),
        );
        let (selection, windows_sandbox_level) =
            crate::sandbox_selection::sandbox_selection(sandbox);
        // Requests use absolute paths. Start at the filesystem root even if the policy cwd was
        // removed, retaining its drive or share on Windows and handling opaque native paths.
        let helper_cwd = sandbox
            .cwd
            .to_abs_path()
            .map_err(|err| invalid_request(err.to_string()))?
            .ancestors()
            .last()
            .ok_or_else(|| invalid_request("filesystem sandbox cwd has no root".to_string()))?;
        let command = SandboxCommand {
            program: helper.as_os_str().to_owned(),
            args: vec![CODEX_FS_HELPER_ARG1.to_string()],
            cwd: PathUri::from_abs_path(&helper_cwd),
            env: self.helper_env.clone(),
            managed_network: None,
            additional_permissions: None,
        };
        let mut request = sandbox_manager
            .transform(SandboxTransformRequest {
                command,
                permissions: &sandbox.permissions,
                workspace_roots: Some(&sandbox.workspace_roots),
                runtime_readable_paths: Some(&helper_readable_paths),
                selection,
                network: Default::default(),
                sandbox_policy_cwd: &sandbox.cwd,
                sandbox_exe: if cfg!(windows) {
                    Some(helper.as_path())
                } else {
                    self.runtime_paths.codex_linux_sandbox_exe.as_deref()
                },
                use_legacy_landlock: sandbox.use_legacy_landlock,
                windows_sandbox_level,
            })
            .map_err(|err| match err {
                codex_sandboxing::SandboxTransformError::SandboxUnavailable => invalid_request(
                    "filesystem sandbox cannot be enforced on this executor".to_string(),
                ),
                codex_sandboxing::SandboxTransformError::InvalidPolicy(err) => {
                    invalid_request(err.to_string())
                }
                err => invalid_request(format!("failed to prepare fs sandbox: {err}")),
            })?
            .into_command_request()
            .map_err(io_error)?;
        request.sandbox_override = sandbox.sandbox_override;
        Ok(request)
    }
}

fn helper_env() -> HashMap<String, String> {
    helper_env_from_vars(std::env::vars_os())
}

fn helper_env_from_vars(
    vars: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> HashMap<String, String> {
    vars.into_iter()
        .filter_map(|(key, value)| {
            let key = key.to_string_lossy();
            helper_env_key_is_allowed(&key)
                .then(|| (key.into_owned(), value.to_string_lossy().into_owned()))
        })
        .collect()
}

fn helper_env_key_is_allowed(key: &str) -> bool {
    FS_HELPER_ENV_ALLOWLIST.contains(&key)
        // CoreFoundation consults this before falling back to user lookup during helper startup.
        || (cfg!(target_os = "macos") && key == "__CF_USER_TEXT_ENCODING")
        || bazel_bwrap_env_key_is_allowed(key)
        // MXC needs SystemDrive to resolve platform directories, and SystemRoot and
        // LOCALAPPDATA to create the sandboxed helper process.
        || (cfg!(windows)
            && ["PATH", "SystemDrive", "SystemRoot", "LOCALAPPDATA"]
                .iter()
                .any(|allowed| key.eq_ignore_ascii_case(allowed)))
}

#[cfg(debug_assertions)]
fn bazel_bwrap_env_key_is_allowed(key: &str) -> bool {
    option_env!("BAZEL_PACKAGE").is_some() && FS_HELPER_BAZEL_BWRAP_ENV_ALLOWLIST.contains(&key)
}

#[cfg(not(debug_assertions))]
fn bazel_bwrap_env_key_is_allowed(_key: &str) -> bool {
    false
}

#[tracing::instrument(name = "fs.sandbox_execute", skip_all)]
async fn run_command(
    command: SandboxExecRequest,
    request_json: Vec<u8>,
) -> Result<FsHelperPayload, JSONRPCErrorError> {
    let mut child = spawn_command(command, ChildStdin::Piped)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| internal_error("failed to open fs sandbox helper stdin".to_string()))?;

    #[cfg(windows)]
    let mut request_json = request_json;
    #[cfg(windows)]
    request_json.push(b'\n');
    stdin.write_all(&request_json).await.map_err(io_error)?;

    #[cfg(windows)]
    let response = {
        stdin.flush().await.map_err(io_error)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| internal_error("failed to open fs sandbox helper stdout".to_string()))?;
        let stderr = drain_helper_stderr(&mut child);
        let response = read_helper_response(stdout).await;
        drop(stdin);
        reap_helper_after_response(child, stderr).await?;
        response?
    };

    #[cfg(not(windows))]
    let response = {
        stdin.shutdown().await.map_err(io_error)?;
        drop(stdin);
        wait_for_helper_output(child).await?.stdout
    };

    let response = serde_json::from_slice(&response).map_err(json_error)?;
    match response {
        FsHelperResponse::Ok(payload) => Ok(payload),
        FsHelperResponse::Error(error) => Err(error),
    }
}




#[cfg(not(windows))]
pub(crate) async fn wait_for_helper_output(
    child: Child,
) -> Result<std::process::Output, JSONRPCErrorError> {
    let output = child.wait_with_output().await.map_err(io_error)?;
    if !output.status.success() {
        return Err(internal_error(format!(
            "fs sandbox helper failed with status {status}: {stderr}",
            status = output.status,
            stderr = String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output)
}

pub(crate) fn spawn_command(
    request: SandboxExecRequest,
    stdin: ChildStdin,
) -> Result<Child, JSONRPCErrorError> {
    let Some((program, args)) = request.program_and_args() else {
        return Err(invalid_request("fs sandbox command was empty".to_string()));
    };
    let mut command = Command::new(program);
    command.args(args);
    #[cfg(unix)]
    if let Some(arg0) = request.arg0 {
        command.arg0(arg0);
    }
    // TODO(anp): Keep PathUri through the filesystem helper launch boundary.
    let cwd = request.cwd.to_abs_path().map_err(io_error)?;
    command.current_dir(cwd.as_path());
    let mut env = request.env;
    env.retain(|name, _| !codex_protocol::shell_environment::is_non_inheritable_env_var(name));
    command.envs(env);
    command.stdin(stdin);
    // A helper is a known executable: native launch errors must not retry through fork.
    command.fallback(SpawnFallback::ReturnError);
    // macOS cannot receive passed fds with close-on-exec set atomically.
    #[cfg(target_os = "macos")]
    command.descriptor_policy(DescriptorPolicy::Explicit);
    command.spawn().map_err(io_error)
}

pub(crate) fn io_error(err: std::io::Error) -> JSONRPCErrorError {
    internal_error(err.to_string())
}

fn json_error(err: serde_json::Error) -> JSONRPCErrorError {
    internal_error(format!(
        "failed to encode or decode fs sandbox helper message: {err}"
    ))
}
