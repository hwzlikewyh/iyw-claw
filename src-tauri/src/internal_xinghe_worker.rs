//! 星河核心在主进程运行，沙箱角色在同一 EXE 的早期入口分流。

use std::path::PathBuf;

pub const RUNTIME_VERSION: &str = "0.156.1";
const RETIRED_WORKER_FLAG: &str = "--internal-xinghe-worker";
const RUNTIME_INFO_FLAG: &str = "--internal-xinghe-runtime-info";
const RUNTIME_MARKER: &str = "IYW_XINGHE_IN_PROCESS_V1";
const UPSTREAM_IDENTITY: &str = include_str!("../../harness/codex/upstream.lock");

pub(crate) fn is_desktop_agent(agent: crate::models::agent::AgentType) -> bool {
    cfg!(feature = "tauri-runtime") && agent == crate::models::agent::AgentType::Codex
}

pub(crate) fn installed_version() -> Option<String> {
    resolve_helper_path().ok().map(|_| RUNTIME_VERSION.to_string())
}

pub(crate) fn installation_available(
    agent: crate::models::agent::AgentType,
    recorded: Option<&str>,
) -> bool {
    if is_desktop_agent(agent) {
        installed_version().is_some()
    } else {
        recorded.is_some_and(|version| !version.trim().is_empty())
    }
}

pub(crate) fn require_external_agent(
    agent: crate::models::agent::AgentType,
) -> Result<(), crate::acp::error::AcpError> {
    if is_desktop_agent(agent) {
        Err(crate::acp::error::AcpError::protocol(
            "星河随应用更新，不支持单独安装、卸载或切换 npm 版本",
        ))
    } else {
        Ok(())
    }
}

/// 必须在 Tauri、日志、凭证和数据库初始化之前调用。
pub fn dispatch_early() -> bool {
    if iyw_codex_harness::dispatch_upstream_helper() {
        return true;
    }
    let first = std::env::args_os().nth(1);
    if first.as_deref() == Some(std::ffi::OsStr::new(RUNTIME_INFO_FLAG)) {
        println!("{}", serde_json::json!({
            "mode": "in-process",
            "marker": RUNTIME_MARKER,
            "version": RUNTIME_VERSION,
            "upstream": serde_json::from_str::<serde_json::Value>(UPSTREAM_IDENTITY)
                .expect("compiled upstream identity must be valid"),
            "helpers": "same-executable",
        }));
        return true;
    }
    if first.as_deref() == Some(std::ffi::OsStr::new(RETIRED_WORKER_FLAG)) {
        eprintln!("内置星河运行时已改为进程内调用");
        std::process::exit(64);
    }
    false
}

pub fn resolve_helper_path() -> Result<PathBuf, String> {
    let executable = crate::update::runtime::self_exe();
    if !executable.is_absolute() || !executable.is_file() {
        return Err("内置星河主程序不可用，请修复安装".into());
    }
    Ok(executable)
}
