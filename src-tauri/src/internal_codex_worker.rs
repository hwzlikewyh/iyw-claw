//! Internal self-reexec entry point for the optional Codex runtime worker.
//!
//! The desktop binary never links Codex. It starts itself with a private flag,
//! then this module loads the separately-built worker library before Tauri or
//! the application database initialize.

use std::ffi::OsStr;
use std::path::PathBuf;

use libloading::Library;

pub const WORKER_FLAG: &str = "--internal-codex-worker";
pub const ACTIVE_ENV: &str = "IYW_CLAW_CODEX_WORKER_ACTIVE";

const WORKER_ENTRY: &[u8] = b"iyw_codex_worker_run_v1\0";
const HELPER_ENTRY: &[u8] = b"iyw_codex_worker_dispatch_helper_v1\0";

/// Handles a worker process or an upstream helper reexec before app startup.
///
/// Returns only for an ordinary application launch. A recognized internal mode
/// always exits with the worker library's status to keep stdout protocol-clean.
pub fn dispatch_early() -> bool {
    let mut args = std::env::args_os();
    let program = args.next();
    let first_argument = args.next();
    if first_argument.as_deref() == Some(OsStr::new(WORKER_FLAG)) {
        exit_worker(WORKER_ENTRY);
    }
    if is_active_worker()
        && is_upstream_helper_invocation(program.as_deref(), first_argument.as_deref())
    {
        exit_worker(HELPER_ENTRY);
    }
    false
}

/// Resolves the private runtime library without accepting a user override.
pub fn resolve_library() -> Result<PathBuf, String> {
    library_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "internal Codex worker library is unavailable".to_string())
}

fn exit_worker(symbol: &[u8]) -> ! {
    let status = load_and_run(symbol).unwrap_or_else(|error| {
        eprintln!("[internal-codex-worker] {error}");
        1
    });
    std::process::exit(status);
}

fn load_and_run(symbol: &[u8]) -> Result<i32, String> {
    let path = resolve_library()?;
    unsafe {
        // The library remains live until its C ABI entry point returns.
        let library = Library::new(&path)
            .map_err(|_| "failed to load internal Codex worker library".to_string())?;
        let entry = library
            .get::<unsafe extern "C" fn() -> i32>(symbol)
            .map_err(|_| "internal Codex worker entry point is unavailable".to_string())?;
        Ok(entry())
    }
}

fn is_active_worker() -> bool {
    std::env::var(ACTIVE_ENV)
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn is_upstream_helper_invocation(program: Option<&OsStr>, first_argument: Option<&OsStr>) -> bool {
    let program_name = program
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .and_then(OsStr::to_str);
    if matches!(
        program_name,
        Some("codex-execve-wrapper")
            | Some("codex-linux-sandbox")
            | Some("apply_patch")
            | Some("applypatch")
    ) {
        return true;
    }
    first_argument.is_some_and(|value| value.to_string_lossy().starts_with("--codex-"))
}

fn library_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(executable) = std::env::current_exe().ok() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(worker_library_filename()));
            candidates.push(
                directory
                    .join("resources/codex-worker")
                    .join(worker_library_filename()),
            );
            candidates.push(
                directory
                    .join("../Resources/codex-worker")
                    .join(worker_library_filename()),
            );
        }
    }
    if cfg!(debug_assertions) {
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/codex-worker")
                .join(worker_library_filename()),
        );
    }
    candidates
}

const fn worker_library_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "iyw_codex_worker.dll"
    } else if cfg!(target_os = "macos") {
        "libiyw_codex_worker.dylib"
    } else {
        "libiyw_codex_worker.so"
    }
}
