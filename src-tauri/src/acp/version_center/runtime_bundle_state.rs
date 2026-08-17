use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::registry;
use crate::models::agent::AgentType;

const REQUIRED_UVX_DIRECTORIES: [&str; 4] = ["cache", "tools", "bin", "python"];
const REQUIRED_UVX_FILES: [&str; 2] = ["iyw-agent-bundle.json", "uv.lock"];

pub(crate) fn uvx_bundle_env(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
) -> Option<BTreeMap<&'static str, PathBuf>> {
    let root = uvx_bundle_root(paths, agent_type, version).ok()?;
    uvx_bundle_ready(&root).then(|| bundle_env(&root))
}

pub(crate) fn activate_uvx_bundle(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    staging: &Path,
    entrypoint: &Path,
) -> Result<PathBuf, AcpError> {
    validate_uvx_layout(staging, entrypoint)?;
    let destination = uvx_bundle_root(paths, agent_type, version)?;
    let parent = destination
        .parent()
        .ok_or_else(|| AcpError::DownloadFailed("uvx bundle destination has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(io_error)?;
    let previous = move_existing(paths, agent_type, &destination)?;
    if let Err(error) = std::fs::rename(staging, &destination) {
        if let Some(previous) = previous.as_ref() {
            let _ = std::fs::rename(previous, &destination);
        }
        return Err(AcpError::DownloadFailed(format!(
            "activate uvx runtime bundle failed: {error}"
        )));
    }
    if let Some(previous) = previous {
        let _ = std::fs::remove_dir_all(previous);
    }
    Ok(destination)
}

pub(crate) fn remove_uvx_bundles(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let root = paths
        .uv_runtime_dir()
        .join("bundles")
        .join(registry::registry_id_for(agent_type));
    match std::fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn uvx_bundle_root(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
) -> Result<PathBuf, AcpError> {
    let version = sanitize_version(version)?;
    Ok(paths
        .uv_runtime_dir()
        .join("bundles")
        .join(registry::registry_id_for(agent_type))
        .join(version)
        .join(registry::current_platform()))
}

fn validate_uvx_layout(root: &Path, entrypoint: &Path) -> Result<(), AcpError> {
    if !uvx_bundle_ready(root) {
        return Err(AcpError::DownloadFailed(
            "uvx runtime bundle layout is incomplete".into(),
        ));
    }
    let relative = entrypoint.strip_prefix(root).map_err(|_| {
        AcpError::DownloadFailed("uvx runtime bundle entrypoint is outside its root".into())
    })?;
    if !relative.starts_with("bin") {
        return Err(AcpError::DownloadFailed(
            "uvx runtime bundle entrypoint is outside the managed bin directory".into(),
        ));
    }
    Ok(())
}

fn uvx_bundle_ready(root: &Path) -> bool {
    REQUIRED_UVX_DIRECTORIES
        .iter()
        .all(|directory| root.join(directory).is_dir())
        && REQUIRED_UVX_FILES
            .iter()
            .all(|file| root.join(file).is_file())
}

fn bundle_env(root: &Path) -> BTreeMap<&'static str, PathBuf> {
    BTreeMap::from([
        ("UV_CACHE_DIR", root.join("cache")),
        ("UV_TOOL_DIR", root.join("tools")),
        ("UV_TOOL_BIN_DIR", root.join("bin")),
        ("UV_PYTHON_INSTALL_DIR", root.join("python")),
    ])
}

fn move_existing(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    destination: &Path,
) -> Result<Option<PathBuf>, AcpError> {
    if !destination.exists() {
        return Ok(None);
    }
    let trash = paths.trash_dir().join("uvx").join(format!(
        "{}-{}",
        registry::registry_id_for(agent_type),
        uuid::Uuid::new_v4()
    ));
    let parent = trash
        .parent()
        .ok_or_else(|| AcpError::DownloadFailed("uvx trash path has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(io_error)?;
    std::fs::rename(destination, &trash).map_err(io_error)?;
    Ok(Some(trash))
}

fn sanitize_version(version: &str) -> Result<&str, AcpError> {
    let version = version.trim();
    let valid = !version.is_empty()
        && !matches!(version, "." | "..")
        && version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+'));
    valid
        .then_some(version)
        .ok_or_else(|| AcpError::DownloadFailed("uvx bundle version is invalid".into()))
}

fn io_error(error: std::io::Error) -> AcpError {
    AcpError::DownloadFailed(error.to_string())
}
