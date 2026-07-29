use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentPointer {
    version: String,
    platform: String,
}

pub fn managed_tool_executable(name: &str) -> Option<PathBuf> {
    let (tool_id, relative) = match name {
        "git" => ("git", Path::new("cmd").join("git.exe")),
        "node" => ("node", PathBuf::from("node.exe")),
        "npm" => ("node", PathBuf::from("npm.cmd")),
        "uv" => ("uv", PathBuf::from("uv.exe")),
        "uvx" => ("uv", PathBuf::from("uvx.exe")),
        _ => return None,
    };
    let data_dir = std::env::var_os("IYW_CLAW_DATA_DIR")?;
    let root = PathBuf::from(data_dir).join("runtime").join(tool_id);
    let raw = std::fs::read_to_string(root.join("current.json")).ok()?;
    let pointer = serde_json::from_str::<CurrentPointer>(&raw).ok()?;
    if semver::Version::parse(&pointer.version).is_err() || pointer.platform != platform_dir_name()
    {
        return None;
    }
    let candidate = root
        .join(pointer.version)
        .join(pointer.platform)
        .join(relative);
    let canonical_root = std::fs::canonicalize(root).ok()?;
    let canonical_candidate = std::fs::canonicalize(&candidate).ok()?;
    (canonical_candidate.starts_with(&canonical_root) && canonical_candidate.is_file())
        .then_some(canonical_candidate)
}

pub fn runtime_dir(
    data_dir: &Path,
    tool_id: &str,
    version: &str,
) -> Result<PathBuf, AppCommandError> {
    let version = semver::Version::parse(version)
        .map_err(|_| AppCommandError::invalid_input("Managed tool version is invalid"))?;
    Ok(data_dir
        .join("runtime")
        .join(tool_id)
        .join(version.to_string())
        .join(platform_dir_name()))
}

pub fn staging_dir(data_dir: &Path, tool_id: &str) -> Result<PathBuf, AppCommandError> {
    if !capability::known_tool(tool_id) {
        return Err(AppCommandError::invalid_input("Unknown managed tool"));
    }
    Ok(data_dir
        .join("runtime")
        .join(tool_id)
        .join(".staging")
        .join(uuid::Uuid::new_v4().to_string()))
}

pub async fn read_current_pointer(
    data_dir: &Path,
    tool_id: &str,
) -> Result<Option<Vec<u8>>, AppCommandError> {
    match tokio::fs::read(pointer_path(data_dir, tool_id)?).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

pub async fn write_current_pointer(
    data_dir: &Path,
    tool_id: &str,
    version: &str,
) -> Result<(), AppCommandError> {
    let value = serde_json::json!({ "version": version, "platform": platform_dir_name() });
    write_pointer_bytes(data_dir, tool_id, value.to_string().as_bytes()).await
}

pub async fn restore_current_pointer(
    data_dir: &Path,
    tool_id: &str,
    previous: Option<Vec<u8>>,
) -> Result<(), AppCommandError> {
    if let Some(bytes) = previous {
        return write_pointer_bytes(data_dir, tool_id, &bytes).await;
    }
    match tokio::fs::remove_file(pointer_path(data_dir, tool_id)?).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

async fn write_pointer_bytes(
    data_dir: &Path,
    tool_id: &str,
    bytes: &[u8],
) -> Result<(), AppCommandError> {
    let pointer = pointer_path(data_dir, tool_id)?;
    let parent = pointer.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Managed tool runtime path is invalid")
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(AppCommandError::io)?;
    let temporary = pointer.with_extension("next");
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&temporary, &pointer)
        .await
        .map_err(AppCommandError::io)
}

fn pointer_path(data_dir: &Path, tool_id: &str) -> Result<PathBuf, AppCommandError> {
    if !capability::known_tool(tool_id) {
        return Err(AppCommandError::invalid_input("Unknown managed tool"));
    }
    Ok(data_dir.join("runtime").join(tool_id).join("current.json"))
}

fn platform_dir_name() -> &'static str {
    match capability::current_arch() {
        "x86_64" => "win-x64",
        "aarch64" => "win-arm64",
        "x86" => "win-x86",
        _ => "unknown",
    }
}
