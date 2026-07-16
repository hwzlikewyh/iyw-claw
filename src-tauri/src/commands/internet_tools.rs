use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::{binary_cache, npm_runtime};

mod types;
pub use types::*;
mod commands;
pub(crate) use commands::*;
mod bootstrap;
pub use bootstrap::bootstrap_core;
mod skills;
use skills::*;

const BOOTSTRAP_MARKER: &str = ".internet-tools-bootstrap.v1";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

fn bootstrap_marker_content() -> String {
    format!("agent-reach={AGENT_REACH_VERSION}\nopencli={OPENCLI_VERSION}\n")
}

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn agent_reach_package_spec() -> String {
    format!(
        "https://github.com/Panniantong/Agent-Reach/archive/refs/tags/v{AGENT_REACH_VERSION}.zip"
    )
}

fn opencli_package_spec() -> String {
    format!("@jackwener/opencli@{OPENCLI_VERSION}")
}

fn mcporter_package_spec() -> String {
    format!("mcporter@{MCPORTER_VERSION}")
}

fn uv_tool_bin_dir(paths: &AgentStoragePaths) -> PathBuf {
    paths.uv_runtime_dir().join("bin")
}

fn opencli_prefix(paths: &AgentStoragePaths) -> PathBuf {
    paths
        .npm_runtime_dir()
        .join("internet-tools")
        .join("opencli")
        .join(OPENCLI_VERSION)
}

fn opencli_command_path(paths: &AgentStoragePaths) -> PathBuf {
    let name = if cfg!(windows) {
        "opencli.cmd"
    } else {
        "opencli"
    };
    npm_runtime::npm_prefix_bin_dir(&opencli_prefix(paths)).join(name)
}

fn npm_tool_command_path(paths: &AgentStoragePaths, command: &str) -> PathBuf {
    let name = if cfg!(windows) {
        format!("{command}.cmd")
    } else {
        command.to_string()
    };
    npm_runtime::npm_prefix_bin_dir(&opencli_prefix(paths)).join(name)
}

fn mcporter_config_path(paths: &AgentStoragePaths) -> PathBuf {
    paths.config_dir().join("internet-tools/mcporter.json")
}

fn agent_reach_command_path(paths: &AgentStoragePaths) -> PathBuf {
    let name = if cfg!(windows) {
        "agent-reach.exe"
    } else {
        "agent-reach"
    };
    uv_tool_bin_dir(paths).join(name)
}

async fn install_agent_reach(paths: &AgentStoragePaths) -> Result<(), String> {
    if let Ok(executable) = std::env::current_exe() {
        binary_cache::seed_bundled_uv_tools(paths, &executable)
            .map_err(|error| error.to_string())?;
    }
    binary_cache::ensure_uv_tool(paths, |message| {
        tracing::info!("[internet-tools] {message}");
    })
    .await
    .map_err(|error| error.to_string())?;
    let uv = binary_cache::find_cached_uv_tool(paths, "uv")
        .ok_or_else(|| "uv missing after runtime installation".to_string())?;
    fs::create_dir_all(uv_tool_bin_dir(paths)).map_err(|error| error.to_string())?;
    let mut command = crate::process::tokio_command(uv);
    command
        .envs(binary_cache::uv_runtime_env(paths))
        .env("UV_TOOL_BIN_DIR", uv_tool_bin_dir(paths))
        .args(["tool", "install", "--force", &agent_reach_package_spec()]);
    run_install_command(command, "Agent Reach").await
}

async fn install_opencli(paths: &AgentStoragePaths) -> Result<(), String> {
    let prefix = opencli_prefix(paths);
    fs::create_dir_all(&prefix).map_err(|error| error.to_string())?;
    fs::create_dir_all(paths.npm_cache_dir()).map_err(|error| error.to_string())?;
    let mut command = crate::process::tokio_command("npm");
    command.args(["install", "--global", "--include=optional", "--prefix"]);
    command
        .arg(&prefix)
        .arg("--cache")
        .arg(paths.npm_cache_dir())
        .arg(opencli_package_spec())
        .arg(mcporter_package_spec());
    run_install_command(command, "OpenCLI and mcporter").await?;
    configure_exa(paths).await
}

async fn configure_exa(paths: &AgentStoragePaths) -> Result<(), String> {
    let config = mcporter_config_path(paths);
    if let Some(parent) = config.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut command = crate::process::tokio_command(npm_tool_command_path(paths, "mcporter"));
    command.env("MCPORTER_CONFIG", &config).args([
        "config",
        "add",
        "exa",
        "https://mcp.exa.ai/mcp",
    ]);
    run_install_command(command, "Exa configuration").await
}

async fn run_install_command(
    mut command: tokio::process::Command,
    name: &str,
) -> Result<(), String> {
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| format!("failed to start {name} installer: {error}"))?;
    let pid = child.id();
    let output = match tokio::time::timeout(INSTALL_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result.map_err(|error| format!("failed to wait for {name}: {error}"))?,
        Err(_) => {
            if let Some(pid) = pid {
                let _ = kill_tree::tokio::kill_tree(pid).await;
            }
            return Err(format!("{name} install timed out after 600 seconds"));
        }
    };
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let tail = detail
        .char_indices()
        .rev()
        .nth(2_000)
        .map(|(index, _)| &detail[index..])
        .unwrap_or(&detail);
    Err(format!("{name} install failed: {tail}"))
}

async fn run_tool_output(
    mut command: tokio::process::Command,
    name: &str,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| format!("failed to start {name}: {error}"))?;
    let pid = child.id();
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(result) => result.map_err(|error| format!("failed to wait for {name}: {error}")),
        Err(_) => {
            if let Some(pid) = pid {
                let _ = kill_tree::tokio::kill_tree(pid).await;
            }
            Err(format!(
                "{name} timed out after {} seconds",
                timeout.as_secs()
            ))
        }
    }
}

fn output_text(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|part| part.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.'))
        .find(|part| {
            part.trim_start_matches(['v', 'V'])
                .split('.')
                .all(|segment| !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit()))
        })
        .map(|part| part.trim_start_matches(['v', 'V']).to_string())
}

async fn detect_tool(paths: &AgentStoragePaths, tool: InternetToolId) -> InternetToolInfo {
    let path = match tool {
        InternetToolId::AgentReach => agent_reach_command_path(paths),
        InternetToolId::Opencli => opencli_command_path(paths),
    };
    let installed = path.is_file();
    let expected = expected_version(tool);
    if !installed {
        return InternetToolInfo {
            id: tool,
            status: InternetToolStatus::NotInstalled,
            installed,
            version: None,
            expected_version: expected.to_string(),
            path: None,
            runtime_error: None,
        };
    }

    let mut command = crate::process::tokio_command(&path);
    command
        .arg("--version")
        .envs(private_tool_environment_for(paths));
    let output = run_tool_output(command, "tool version check", Duration::from_secs(20)).await;
    let (version, runtime_error) = match output {
        Ok(output) if output.status.success() => (parse_version(&output_text(&output)), None),
        Ok(output) => (None, Some(output_text(&output))),
        Err(error) => (None, Some(error)),
    };
    InternetToolInfo {
        id: tool,
        status: tool_status(
            installed,
            version.as_deref(),
            expected,
            runtime_error.as_deref(),
        ),
        installed,
        version,
        expected_version: expected.to_string(),
        path: Some(path.to_string_lossy().to_string()),
        runtime_error,
    }
}

pub(crate) fn private_tool_bin_dirs() -> Vec<PathBuf> {
    let Some(paths) = AgentStoragePaths::active() else {
        return Vec::new();
    };
    private_tool_bin_dirs_for(&paths)
}

fn private_tool_bin_dirs_for(paths: &AgentStoragePaths) -> Vec<PathBuf> {
    [
        binary_cache::uv_tool_dir_for(paths),
        uv_tool_bin_dir(paths),
        npm_runtime::npm_prefix_bin_dir(&opencli_prefix(paths)),
    ]
    .into_iter()
    .collect()
}

pub(crate) fn private_tool_environment() -> Vec<(&'static str, PathBuf)> {
    let Some(paths) = AgentStoragePaths::active() else {
        return Vec::new();
    };
    private_tool_environment_for(&paths)
}

fn private_tool_environment_for(paths: &AgentStoragePaths) -> Vec<(&'static str, PathBuf)> {
    let mut environment = binary_cache::uv_runtime_env(paths)
        .into_iter()
        .collect::<Vec<_>>();
    environment.push(("MCPORTER_CONFIG", mcporter_config_path(paths)));
    environment
}
