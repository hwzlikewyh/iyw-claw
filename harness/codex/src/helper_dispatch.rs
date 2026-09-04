//! Early dispatch for the small set of App Server child helper modes.
//!
//! This deliberately does not invoke Codex CLI or TUI startup. The embedded
//! runtime can re-exec the host executable for sandbox and file helpers, so
//! these handlers must run before the host creates threads or UI state.

use std::ffi::OsStr;

use codex_apply_patch::CODEX_CORE_APPLY_PATCH_ARG1;
#[cfg(unix)]
use codex_exec_server::{run_arg0_exec_helper_main, CODEX_ARG0_EXEC_HELPER_ARG1};
use codex_exec_server::{run_fs_helper_main, CODEX_FS_HELPER_ARG1, LOCAL_FS};

#[cfg(target_os = "windows")]
use codex_windows_sandbox::{run_windows_sandbox_wrapper_main, CODEX_WINDOWS_SANDBOX_ARG1};

/// Returns `false` during a normal iyw-claw startup. A recognized helper mode
/// never returns because the helper owns the child process exit status.
pub fn dispatch_from_process_args() -> bool {
    let mut args = std::env::args_os();
    let program = args.next();
    let argv1 = args.next();
    match helper_mode(program.as_deref(), argv1.as_deref()) {
        #[cfg(unix)]
        Some(HelperMode::UnixExecveWrapper) => run_arg0_exec_helper_main(),
        #[cfg(unix)]
        Some(HelperMode::UnixApplyPatch) => codex_apply_patch::main(),
        #[cfg(target_os = "linux")]
        Some(HelperMode::LinuxSandbox) => codex_linux_sandbox::run_main(),
        #[cfg(unix)]
        Some(HelperMode::ExecHelper) => run_arg0_exec_helper_main(),
        Some(HelperMode::FileSystemHelper) => run_fs_helper_main(),
        Some(HelperMode::ApplyPatchHelper) => run_apply_patch(args.next()),
        #[cfg(target_os = "windows")]
        Some(HelperMode::WindowsSandbox) => run_windows_sandbox_wrapper_main(),
        None => false,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum HelperMode {
    #[cfg(unix)]
    UnixExecveWrapper,
    #[cfg(unix)]
    UnixApplyPatch,
    #[cfg(target_os = "linux")]
    LinuxSandbox,
    #[cfg(unix)]
    ExecHelper,
    FileSystemHelper,
    ApplyPatchHelper,
    #[cfg(target_os = "windows")]
    WindowsSandbox,
}

fn helper_mode(program: Option<&OsStr>, value: Option<&OsStr>) -> Option<HelperMode> {
    #[cfg(not(unix))]
    let _ = program;
    #[cfg(unix)]
    let program_name = program
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .and_then(OsStr::to_str);
    #[cfg(unix)]
    if program_name == Some("codex-execve-wrapper") {
        return Some(HelperMode::UnixExecveWrapper);
    }
    #[cfg(unix)]
    if program_name == Some("apply_patch") || program_name == Some("applypatch") {
        return Some(HelperMode::UnixApplyPatch);
    }
    #[cfg(target_os = "linux")]
    if program_name == Some(codex_sandboxing::landlock::CODEX_LINUX_SANDBOX_ARG0) {
        return Some(HelperMode::LinuxSandbox);
    }
    match value {
        #[cfg(unix)]
        Some(value) if value == OsStr::new(CODEX_ARG0_EXEC_HELPER_ARG1) => {
            Some(HelperMode::ExecHelper)
        }
        Some(value) if value == OsStr::new(CODEX_FS_HELPER_ARG1) => {
            Some(HelperMode::FileSystemHelper)
        }
        Some(value) if value == OsStr::new(CODEX_CORE_APPLY_PATCH_ARG1) => {
            Some(HelperMode::ApplyPatchHelper)
        }
        #[cfg(target_os = "windows")]
        Some(value) if value == OsStr::new(CODEX_WINDOWS_SANDBOX_ARG1) => {
            Some(HelperMode::WindowsSandbox)
        }
        _ => None,
    }
}

fn run_apply_patch(patch: Option<std::ffi::OsString>) -> ! {
    let patch = match patch.and_then(|value| value.into_string().ok()) {
        Some(value) => value,
        None => {
            eprintln!("Codex apply-patch helper requires a UTF-8 patch argument.");
            std::process::exit(2);
        }
    };
    let cwd = match codex_utils_absolute_path::AbsolutePathBuf::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("Codex apply-patch helper cannot resolve its working directory: {error}");
            std::process::exit(1);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Codex apply-patch helper cannot start its runtime: {error}");
            std::process::exit(1);
        }
    };
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let result = runtime.block_on(codex_apply_patch::apply_patch_with_options(
        &patch,
        codex_apply_patch::ApplyPatchOptions {
            update_file_mode: codex_apply_patch::apply_patch_file_update_mode_from_env(),
            ..Default::default()
        },
        &codex_utils_path_uri::PathUri::from_abs_path(&cwd),
        &mut stdout,
        &mut stderr,
        LOCAL_FS.as_ref(),
        None,
    ));
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_flag_dispatched_helpers() {
        assert_eq!(
            helper_mode(None, Some(OsStr::new(CODEX_FS_HELPER_ARG1))),
            Some(HelperMode::FileSystemHelper)
        );
        assert_eq!(
            helper_mode(None, Some(OsStr::new(CODEX_CORE_APPLY_PATCH_ARG1))),
            Some(HelperMode::ApplyPatchHelper)
        );
        assert_eq!(helper_mode(None, Some(OsStr::new("ordinary-flag"))), None);
    }

    #[cfg(unix)]
    #[test]
    fn recognizes_unix_arg0_helpers_without_consuming_argv1() {
        assert_eq!(
            helper_mode(
                Some(OsStr::new("/tmp/codex-execve-wrapper")),
                Some(OsStr::new("program")),
            ),
            Some(HelperMode::UnixExecveWrapper)
        );
        assert_eq!(
            helper_mode(
                Some(OsStr::new("/tmp/apply_patch")),
                Some(OsStr::new("patch body")),
            ),
            Some(HelperMode::UnixApplyPatch)
        );
        assert_eq!(
            helper_mode(
                Some(OsStr::new("/tmp/applypatch")),
                Some(OsStr::new("patch body")),
            ),
            Some(HelperMode::UnixApplyPatch)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn recognizes_linux_sandbox_by_argv0() {
        assert_eq!(
            helper_mode(
                Some(OsStr::new("/tmp/codex-linux-sandbox")),
                Some(OsStr::new("--policy")),
            ),
            Some(HelperMode::LinuxSandbox)
        );
    }
}
