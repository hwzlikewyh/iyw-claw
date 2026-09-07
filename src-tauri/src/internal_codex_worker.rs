//! Internal self-reexec entry point for the optional Codex runtime worker.
//!
//! The desktop binary never links Codex. It starts itself with a private flag,
//! then this module loads the separately-built worker library before Tauri or
//! the application database initialize.

use std::ffi::OsStr;
use std::path::PathBuf;

use libloading::Library;

#[path = "internal_codex_worker_identity.rs"]
mod identity;

#[cfg(all(feature = "tauri-runtime", target_os = "linux"))]
#[path = "internal_codex_worker_resources.rs"]
mod resources;

pub const WORKER_FLAG: &str = "--internal-codex-worker";
pub const ACTIVE_ENV: &str = "IYW_CLAW_CODEX_WORKER_ACTIVE";
pub const RUNTIME_VERSION: &str = "0.153.4";

pub(crate) fn is_desktop_agent(agent: crate::models::agent::AgentType) -> bool {
    cfg!(feature = "tauri-runtime") && agent == crate::models::agent::AgentType::Codex
}

pub(crate) fn installed_version() -> Option<String> {
    resolve_library().ok().map(|_| RUNTIME_VERSION.to_string())
}

pub(crate) fn installation_available(agent: crate::models::agent::AgentType, recorded: Option<&str>) -> bool {
    if is_desktop_agent(agent) { installed_version().is_some() }
    else { recorded.is_some_and(|version| !version.trim().is_empty()) }
}

pub(crate) fn require_external_agent(agent: crate::models::agent::AgentType) -> Result<(), crate::acp::error::AcpError> {
    if is_desktop_agent(agent) {
        Err(crate::acp::error::AcpError::protocol("星河随应用更新，不支持单独安装、卸载或切换 npm 版本"))
    } else { Ok(()) }
}

const WORKER_ENTRY: &[u8] = b"iyw_codex_worker_run_v1\0";
const HELPER_ENTRY: &[u8] = b"iyw_codex_worker_dispatch_helper_v1\0";
const ABI_ENTRY: &[u8] = b"iyw_codex_worker_abi_version\0";
const CORE_VERSION_ENTRY: &[u8] = b"iyw_codex_worker_core_version\0";
const REQUIRED_ABI: u64 = 1;
const REQUIRED_CORE_VERSION: u64 = 153 * 1_000 + 4;

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
    let library = library_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "内置星河运行时 is not installed，请修复安装".to_string())?;
    identity::validate(&library)?;
    if !library.with_file_name(helper_filename()).is_file() {
        return Err("内置星河文件辅助程序不完整，请修复安装".into());
    }
    if cfg!(target_os = "windows") {
        for helper in ["codex-windows-sandbox-setup.exe", "codex-command-runner.exe"] {
            if !library.with_file_name(helper).is_file() {
                return Err("内置星河沙箱辅助程序不完整，请修复安装".into());
            }
        }
    }
    Ok(library)
}

pub(crate) const fn helper_filename() -> &'static str {
    if cfg!(target_os = "windows") { "iyw-codex-helper.exe" } else { "iyw-codex-helper" }
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
        let abi = library.get::<unsafe extern "C" fn() -> u64>(ABI_ENTRY)
            .map_err(|_| "内置星河运行时缺少版本接口，请修复安装".to_string())?;
        let core_version = library.get::<unsafe extern "C" fn() -> u64>(CORE_VERSION_ENTRY)
            .map_err(|_| "内置星河运行时缺少核心版本，请修复安装".to_string())?;
        if abi() != REQUIRED_ABI || core_version() != REQUIRED_CORE_VERSION {
            return Err("内置星河运行时与应用版本不匹配，请修复安装".to_string());
        }
        for required in [WORKER_ENTRY, HELPER_ENTRY] {
            library.get::<unsafe extern "C" fn() -> i32>(required)
                .map_err(|_| "内置星河运行时接口不完整，请修复安装".to_string())?;
        }
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
    #[cfg(all(feature = "tauri-runtime", target_os = "linux"))]
    if let Some(root) = resources::resource_root() {
        candidates.push(root.join("codex-resources").join(worker_library_filename()));
    }
    if let Some(executable) = std::env::current_exe().ok() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(worker_library_filename()));
            candidates.push(directory.join("codex-resources").join(worker_library_filename()));
            candidates.push(directory.join("../Resources/codex-resources").join(worker_library_filename()));
            candidates.push(
                directory
                    .join("resources/codex-worker")
                    .join(worker_library_filename()),
            );
            candidates.push(
                directory
                    .join("../Resources/resources/codex-worker")
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
