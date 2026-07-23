use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::{npm_runtime, registry};
use crate::app_error::AppCommandError;

use super::MCP_SERVER_ID;

pub(super) const PACKAGE_VERSION: &str = "0.2.1";
const PACKAGE_SPEC: &str = "open-computer-use@0.2.1";
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

fn tool_root(paths: &AgentStoragePaths) -> PathBuf {
    paths.npm_runtime_dir().join("tools").join(MCP_SERVER_ID)
}

fn install_prefix(paths: &AgentStoragePaths) -> PathBuf {
    tool_root(paths)
        .join(PACKAGE_VERSION)
        .join(registry::current_platform())
}

pub(super) fn command_is_managed(paths: &AgentStoragePaths, command: &str) -> bool {
    Path::new(command).starts_with(tool_root(paths))
}

fn staging_prefix(paths: &AgentStoragePaths) -> PathBuf {
    paths
        .staging_dir()
        .join(format!("npm-tool-{MCP_SERVER_ID}-{}", uuid::Uuid::new_v4()))
}

fn command_path(prefix: &Path) -> PathBuf {
    let command = if cfg!(windows) {
        "open-computer-use.cmd"
    } else {
        "open-computer-use"
    };
    npm_runtime::npm_prefix_bin_dir(prefix).join(command)
}

fn failure_detail(output: &std::process::Output) -> String {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let detail = combined.trim();
    detail
        .char_indices()
        .rev()
        .nth(2_000)
        .map(|(index, _)| detail[index..].to_string())
        .unwrap_or_else(|| detail.to_string())
}

async fn wait_for_install(child: tokio::process::Child) -> Result<(), AppCommandError> {
    let pid = child.id();
    let output = match tokio::time::timeout(INSTALL_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result.map_err(AppCommandError::io)?,
        Err(_) => {
            if let Some(pid) = pid {
                let _ = kill_tree::tokio::kill_tree(pid).await;
            }
            tracing::warn!(
                timeout_secs = INSTALL_TIMEOUT.as_secs(),
                "[computer-use] private install timed out"
            );
            return Err(AppCommandError::task_execution_failed(
                "Open Computer Use install timed out",
            ));
        }
    };
    if output.status.success() {
        return Ok(());
    }
    Err(
        AppCommandError::task_execution_failed("Open Computer Use install failed")
            .with_detail(failure_detail(&output)),
    )
}

async fn run_private_install(
    paths: &AgentStoragePaths,
    prefix: &Path,
) -> Result<(), AppCommandError> {
    std::fs::create_dir_all(prefix).map_err(AppCommandError::io)?;
    std::fs::create_dir_all(paths.npm_cache_dir()).map_err(AppCommandError::io)?;
    let args =
        npm_runtime::private_npm_install_args(prefix, &paths.npm_cache_dir(), &[PACKAGE_SPEC])
            .map_err(|error| {
                AppCommandError::configuration_invalid("Open Computer Use npm install is invalid")
                    .with_detail(error.to_string())
            })?;
    tracing::info!(
        package = PACKAGE_SPEC,
        prefix = %prefix.display(),
        "[computer-use] private install started"
    );
    let mut command = crate::process::tokio_command("npm");
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = command.spawn().map_err(|error| {
        AppCommandError::dependency_missing("npm is unavailable for Open Computer Use")
            .with_detail(error.to_string())
    })?;
    wait_for_install(child).await
}

fn activate_install(
    paths: &AgentStoragePaths,
    staging: &Path,
    destination: &Path,
) -> Result<(), AppCommandError> {
    let parent = destination.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Open Computer Use install path has no parent")
    })?;
    std::fs::create_dir_all(parent).map_err(AppCommandError::io)?;
    let previous = destination.exists().then(|| {
        paths
            .trash_dir()
            .join("npm-tools")
            .join(format!("{MCP_SERVER_ID}-{}", uuid::Uuid::new_v4()))
    });
    if let Some(previous) = previous.as_ref() {
        let trash_parent = previous.parent().ok_or_else(|| {
            AppCommandError::configuration_invalid("Open Computer Use backup path has no parent")
        })?;
        std::fs::create_dir_all(trash_parent).map_err(AppCommandError::io)?;
        std::fs::rename(destination, previous).map_err(AppCommandError::io)?;
    }
    if let Err(error) = std::fs::rename(staging, destination) {
        if let Some(previous) = previous.as_ref() {
            let _ = std::fs::rename(previous, destination);
        }
        return Err(AppCommandError::io(error));
    }
    if let Some(previous) = previous {
        let _ = std::fs::remove_dir_all(previous);
    }
    Ok(())
}

pub(super) async fn ensure_private_package(
    paths: &AgentStoragePaths,
) -> Result<(PathBuf, bool), AppCommandError> {
    let prefix = install_prefix(paths);
    let executable = command_path(&prefix);
    if executable.is_file() {
        return Ok((executable, false));
    }

    let staging = staging_prefix(paths);
    let staged_executable = command_path(&staging);
    let result = async {
        run_private_install(paths, &staging).await?;
        if !staged_executable.is_file() {
            return Err(AppCommandError::task_execution_failed(
                "Open Computer Use install did not produce its command",
            ));
        }
        activate_install(paths, &staging, &prefix)?;
        Ok(())
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    if !executable.is_file() {
        return Err(AppCommandError::task_execution_failed(
            "Open Computer Use activation did not preserve its command",
        ));
    }
    Ok((executable, true))
}
