use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
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

/// Oldest Node major the packaged internet tools run on.
///
/// mcporter's CLI uses logical-assignment syntax (`??=`, Node 15+) and both it
/// and opencli ship ESM entrypoints that assume a modern loader. On an older
/// Node the failure surfaces as a bare `SyntaxError` from
/// `internal/modules/esm/translators.js` that names neither Node nor its
/// version, so the floor is checked up front instead.
const MIN_NODE_MAJOR: u32 = 18;

fn bootstrap_marker_content() -> String {
    format!("agent-reach={AGENT_REACH_VERSION}\nopencli={OPENCLI_VERSION}\n")
}

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn is_internet_tool_skill_id(id: &str) -> bool {
    id == "agent-reach" || id.starts_with("opencli-")
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

/// Directories holding the managed Node runtime, if it is installed.
fn managed_node_dirs() -> Vec<PathBuf> {
    ["node", "npm"]
        .into_iter()
        .filter_map(crate::acp::version_center::managed_tool_executable)
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect()
}

fn path_dedup_key(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.into_owned()
    }
}

/// Pin the managed Node runtime ahead of the ambient PATH for a child process.
///
/// Why: `opencli.cmd` and `mcporter.cmd` are npm shims that resolve `node` from
/// PATH at run time, so they bind to whatever Node the process PATH happens to
/// expose. `process::ensure_node_in_path` only prepends the managed runtime at
/// startup — when the runtime bootstrap installs Node *after* that (or fails
/// outright), the already-computed PATH still points at a stale system Node and
/// the shims silently inherit it. Re-resolving per spawn keeps the two orders
/// from mattering.
fn apply_managed_node_path(command: &mut tokio::process::Command) {
    let mut directories = managed_node_dirs();
    if directories.is_empty() {
        return;
    }
    let existing = std::env::var_os("PATH").unwrap_or_default();
    directories.extend(std::env::split_paths(&existing));
    let mut seen = BTreeSet::new();
    directories.retain(|path| seen.insert(path_dedup_key(path)));
    if let Ok(joined) = std::env::join_paths(directories) {
        command.env("PATH", joined);
    }
}

/// The `npm` to install with: the managed runtime when present, otherwise the
/// ambient one. Resolved per call rather than via bare `"npm"` so an install
/// that runs after the bootstrap activated Node still picks it up.
fn npm_program() -> OsString {
    crate::acp::version_center::managed_tool_executable("npm")
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("npm"))
}

/// Fail with an actionable message when no Node new enough to run the packaged
/// tools is reachable, instead of letting the shims die on a bare `SyntaxError`.
async fn ensure_node_supported() -> Result<(), String> {
    let program = crate::acp::version_center::managed_tool_executable("node")
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("node"));
    let mut command = crate::process::tokio_command(&program);
    command.arg("--version");
    apply_managed_node_path(&mut command);
    let output = run_tool_output(command, "Node version check", Duration::from_secs(20))
        .await
        .map_err(|error| {
            format!("Node.js is required by the internet tools but could not be run: {error}")
        })?;
    if !output.status.success() {
        return Err(format!(
            "Node.js is required by the internet tools but could not be run: {}",
            output_text(&output)
        ));
    }
    let raw = output_text(&output);
    let version = parse_version(&raw)
        .ok_or_else(|| format!("Could not parse the Node.js version from {raw:?}"))?;
    let major = version
        .split('.')
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or_else(|| format!("Could not parse the Node.js version from {raw:?}"))?;
    if major >= MIN_NODE_MAJOR {
        return Ok(());
    }
    Err(format!(
        "Node.js {version} is too old for the internet tools (need {MIN_NODE_MAJOR} or newer). \
         The managed Node runtime is missing, so a system Node was used instead — \
         finish desktop runtime initialization, or upgrade the system Node."
    ))
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

    // `uv` fetches the archive itself, so acceleration has to be baked into the
    // spec handed to it: gh-proxy candidates first, direct GitHub last.
    //
    // Retrying the whole `uv tool install` is a blunt instrument — a failure that
    // is not network-related (a broken sdist build, say) is deterministic and will
    // repeat against every candidate, multiplying the wait. Distinguishing the two
    // from outside `uv` means string-matching its stderr, which is more fragile
    // than the extra attempts are expensive.
    let candidates = crate::github_mirror::download_candidates(&agent_reach_package_spec());
    let total = candidates.len();
    let mut last_error = String::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let source = index + 1;
        tracing::info!(
            source,
            source_total = total,
            source_host = %crate::github_mirror::host_of(candidate),
            "[internet-tools] installing Agent Reach"
        );
        let mut command = crate::process::tokio_command(uv.clone());
        command
            .envs(binary_cache::uv_runtime_env(paths))
            .env("UV_TOOL_BIN_DIR", uv_tool_bin_dir(paths))
            .args(["tool", "install", "--force", candidate.as_str()]);
        match run_install_command(command, "Agent Reach").await {
            Ok(()) => return Ok(()),
            Err(error) => {
                tracing::warn!(
                    source,
                    source_total = total,
                    source_host = %crate::github_mirror::host_of(candidate),
                    error_detail_present = true,
                    "[internet-tools] Agent Reach source failed"
                );
                last_error = error;
            }
        }
    }
    Err(if last_error.is_empty() {
        "Agent Reach install failed: no download source available".to_string()
    } else {
        last_error
    })
}

async fn install_opencli(paths: &AgentStoragePaths) -> Result<(), String> {
    ensure_node_supported().await?;
    let prefix = opencli_prefix(paths);
    fs::create_dir_all(&prefix).map_err(|error| error.to_string())?;
    fs::create_dir_all(paths.npm_cache_dir()).map_err(|error| error.to_string())?;
    let mut command = crate::process::tokio_command(npm_program());
    command.args(["install", "--global", "--include=optional", "--prefix"]);
    command
        .arg(&prefix)
        .arg("--cache")
        .arg(paths.npm_cache_dir())
        // Route through the same registry as every other managed install
        // (npmmirror by default) instead of npm's built-in registry.npmjs.org.
        .arg(npm_runtime::npm_registry_arg().map_err(|error| error.to_string())?)
        .arg(opencli_package_spec())
        .arg(mcporter_package_spec());
    apply_managed_node_path(&mut command);
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
    apply_managed_node_path(&mut command);
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
    apply_managed_node_path(&mut command);
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
