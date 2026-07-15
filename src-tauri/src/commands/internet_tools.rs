use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::{binary_cache, npm_runtime};
use crate::commands::experts::central_experts_dir;

const AGENT_REACH_VERSION: &str = "1.5.0";
const OPENCLI_VERSION: &str = "1.8.6";
const MCPORTER_VERSION: &str = "0.9.0";
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
    let uv = binary_cache::ensure_uv_tool(paths, |message| {
        tracing::info!("[internet-tools] {message}");
    })
    .await
    .map_err(|error| error.to_string())?;
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
    command.kill_on_drop(true);
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    let tail = detail
        .char_indices()
        .rev()
        .nth(2_000)
        .map(|(index, _)| &detail[index..])
        .unwrap_or(detail);
    Err(format!("{name} install failed: {tail}"))
}

fn find_agent_reach_skill(paths: &AgentStoragePaths) -> Option<PathBuf> {
    walkdir::WalkDir::new(paths.uv_runtime_dir().join("tools"))
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("skill")
                && path.join("SKILL.md").is_file()
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    == Some("agent_reach")
        })
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), String> {
    if target.exists() || fs::symlink_metadata(target).is_ok() {
        crate::commands::acp::remove_skill_entry(target).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let destination = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn sync_packaged_skills(
    agent_reach_skill: &Path,
    opencli_skills: &Path,
    central: &Path,
) -> Result<Vec<String>, String> {
    fs::create_dir_all(central).map_err(|error| error.to_string())?;
    copy_dir(agent_reach_skill, &central.join("agent-reach"))?;
    let mut synced = vec!["agent-reach".to_string()];
    for entry in fs::read_dir(opencli_skills).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let id = entry.file_name().to_string_lossy().to_string();
        if !id.starts_with("opencli-") || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        copy_dir(&entry.path(), &central.join(&id))?;
        synced.push(id);
    }
    Ok(synced)
}

fn bootstrap_is_complete(paths: &AgentStoragePaths) -> bool {
    let opencli_skills = opencli_prefix(paths).join("node_modules/@jackwener/opencli/skills");
    fs::read_to_string(paths.root().join(BOOTSTRAP_MARKER))
        .ok()
        .as_deref()
        == Some(bootstrap_marker_content().as_str())
        && agent_reach_command_path(paths).is_file()
        && opencli_command_path(paths).is_file()
        && npm_tool_command_path(paths, "mcporter").is_file()
        && mcporter_config_path(paths).is_file()
        && central_experts_dir().join("agent-reach/SKILL.md").is_file()
        && packaged_opencli_skills_complete(&opencli_skills, &central_experts_dir())
}

fn packaged_opencli_skills_complete(source: &Path, central: &Path) -> bool {
    let Ok(entries) = fs::read_dir(source) else {
        return false;
    };
    let mut found = false;
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        if !id.starts_with("opencli-") || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        found = true;
        if !central.join(id).join("SKILL.md").is_file() {
            return false;
        }
    }
    found
}

pub async fn bootstrap_core() -> Result<usize, String> {
    let _guard = bootstrap_lock().lock().await;
    let Some(paths) = AgentStoragePaths::active() else {
        return Ok(0);
    };
    if bootstrap_is_complete(&paths) {
        crate::commands::acp::reconcile_shared_market_skills()
            .map_err(|error| error.to_string())?;
        return Ok(0);
    }
    install_agent_reach(&paths).await?;
    install_opencli(&paths).await?;
    let agent_reach_skill = find_agent_reach_skill(&paths)
        .ok_or_else(|| "Agent Reach packaged skill was not found".to_string())?;
    let opencli_skills = opencli_prefix(&paths).join("node_modules/@jackwener/opencli/skills");
    let synced = sync_packaged_skills(&agent_reach_skill, &opencli_skills, &central_experts_dir())?;
    crate::commands::acp::publish_shared_market_skill_ids(&synced)
        .map_err(|error| error.to_string())?;
    fs::write(
        paths.root().join(BOOTSTRAP_MARKER),
        bootstrap_marker_content(),
    )
    .map_err(|error| error.to_string())?;
    Ok(synced.len())
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

#[cfg(test)]
#[path = "internet_tools_tests.rs"]
mod tests;
