use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::registry;
use crate::models::agent::AgentType;

const NPM_OFFICIAL_REGISTRY: &str = "https://registry.npmjs.org";

pub fn private_npm_prefix(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
) -> Result<PathBuf, AcpError> {
    let version = version
        .trim()
        .strip_prefix(['v', 'V'])
        .unwrap_or(version.trim())
        .trim();
    if version.is_empty()
        || !version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+'))
        || matches!(version, "." | "..")
    {
        return Err(AcpError::DownloadFailed(
            "npm runtime version is invalid".to_string(),
        ));
    }
    Ok(paths
        .npm_runtime_dir()
        .join(registry::registry_id_for(agent_type))
        .join(version)
        .join(registry::current_platform()))
}

pub fn npm_prefix_bin_dir(prefix: &Path) -> PathBuf {
    if cfg!(windows) {
        prefix.to_path_buf()
    } else {
        prefix.join("bin")
    }
}

pub fn private_npm_install_args(prefix: &Path, cache: &Path, packages: &[&str]) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("install"),
        OsString::from("--global"),
        OsString::from("--include=optional"),
        OsString::from(format!("--registry={NPM_OFFICIAL_REGISTRY}")),
        path_arg("--prefix=", prefix),
        path_arg("--cache=", cache),
    ];
    args.extend(packages.iter().map(OsString::from));
    args
}

fn path_arg(name: &str, path: &Path) -> OsString {
    let mut value = OsString::from(name);
    value.push(path.as_os_str());
    value
}

pub fn resolve_private_npm_command(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    command: &str,
) -> Option<PathBuf> {
    let prefix = private_npm_prefix(paths, agent_type, version).ok()?;
    resolve_npm_command_from_prefix(&prefix, command)
}

pub fn preferred_private_npm_command_path(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    command: &str,
) -> Result<PathBuf, AcpError> {
    let prefix = private_npm_prefix(paths, agent_type, version)?;
    let bin_dir = npm_prefix_bin_dir(&prefix);
    if cfg!(windows) {
        Ok(bin_dir.join(format!("{command}.cmd")))
    } else {
        Ok(bin_dir.join(command))
    }
}

pub fn private_npm_staging_prefix(paths: &AgentStoragePaths, agent_type: AgentType) -> PathBuf {
    paths.staging_dir().join(format!(
        "npm-{}-{}",
        registry::registry_id_for(agent_type),
        uuid::Uuid::new_v4()
    ))
}

fn resolve_npm_command_from_prefix(prefix: &Path, command: &str) -> Option<PathBuf> {
    let bin_dir = npm_prefix_bin_dir(prefix);

    #[cfg(windows)]
    let candidates = [
        bin_dir.join(format!("{command}.cmd")),
        bin_dir.join(format!("{command}.exe")),
        bin_dir.join(command),
    ];
    #[cfg(not(windows))]
    let candidates = [bin_dir.join(command)];

    candidates
        .into_iter()
        .find(|candidate| is_command_candidate(candidate))
}

pub fn activate_private_npm_runtime(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    staging_prefix: &Path,
    required_commands: &[&str],
) -> Result<PathBuf, AcpError> {
    for command in required_commands {
        if resolve_npm_command_from_prefix(staging_prefix, command).is_none() {
            let _ = std::fs::remove_dir_all(staging_prefix);
            return Err(AcpError::DownloadFailed(format!(
                "private npm install did not produce command '{command}'"
            )));
        }
    }

    let final_prefix = match private_npm_prefix(paths, agent_type, version) {
        Ok(prefix) => prefix,
        Err(error) => {
            let _ = std::fs::remove_dir_all(staging_prefix);
            return Err(error);
        }
    };
    if let Err(error) = activate_staged_prefix(paths, staging_prefix, &final_prefix, agent_type) {
        let _ = std::fs::remove_dir_all(staging_prefix);
        return Err(error);
    }
    Ok(final_prefix)
}

fn activate_staged_prefix(
    paths: &AgentStoragePaths,
    staging_prefix: &Path,
    final_prefix: &Path,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let parent = final_prefix
        .parent()
        .ok_or_else(|| AcpError::DownloadFailed("private npm destination has no parent".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AcpError::DownloadFailed(format!("create npm runtime dir failed: {e}")))?;

    let previous = move_existing_to_trash(paths, final_prefix, agent_type)?;
    if let Err(error) = std::fs::rename(staging_prefix, final_prefix) {
        if let Some(previous) = previous.as_ref() {
            let _ = std::fs::rename(previous, final_prefix);
        }
        return Err(AcpError::DownloadFailed(format!(
            "activate private npm runtime failed: {error}"
        )));
    }
    if let Some(previous) = previous {
        let _ = std::fs::remove_dir_all(previous);
    }
    Ok(())
}

fn move_existing_to_trash(
    paths: &AgentStoragePaths,
    existing: &Path,
    agent_type: AgentType,
) -> Result<Option<PathBuf>, AcpError> {
    if !existing.exists() {
        return Ok(None);
    }
    let trash = paths.trash_dir().join("npm").join(format!(
        "{}-{}",
        registry::registry_id_for(agent_type),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(trash.parent().unwrap())
        .map_err(|e| AcpError::DownloadFailed(format!("create npm trash dir failed: {e}")))?;
    std::fs::rename(existing, &trash)
        .map_err(|e| AcpError::DownloadFailed(format!("move npm runtime aside failed: {e}")))?;
    Ok(Some(trash))
}

pub fn uninstall_private_npm_runtime(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let agent_dir = paths
        .npm_runtime_dir()
        .join(registry::registry_id_for(agent_type));
    if !agent_dir.exists() || std::fs::remove_dir_all(&agent_dir).is_ok() {
        return Ok(());
    }
    let aside = move_existing_to_trash(paths, &agent_dir, agent_type)?.ok_or_else(|| {
        AcpError::DownloadFailed("private npm runtime disappeared during uninstall".into())
    })?;
    let _ = std::fs::remove_dir_all(aside);
    Ok(())
}

pub fn sweep_trash(paths: &AgentStoragePaths) {
    let Ok(entries) = std::fs::read_dir(paths.trash_dir().join("npm")) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_dir_all(entry.path());
    }
}

#[cfg(windows)]
fn is_command_candidate(path: &Path) -> bool {
    path.is_file()
}

#[cfg(not(windows))]
fn is_command_candidate(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(test)]
#[path = "npm_runtime_tests.rs"]
mod tests;
