use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedNodeState {
    version: String,
    platform: String,
}

pub(super) fn managed_node_bin_dir(executable: &Path) -> Option<PathBuf> {
    if let Some(candidate) = crate::acp::version_center::managed_tool_executable("node")
        .and_then(|path| path.parent().map(ToOwned::to_owned))
    {
        return Some(candidate);
    }
    if let Some(install_root) = std::env::var_os(crate::desktop_bootstrap::INSTALL_ROOT_ENV) {
        if let Some(candidate) = managed_node_bin_dir_from_data_root(Path::new(&install_root)) {
            return Some(candidate);
        }
    }
    if let Some(storage_root) = std::env::var_os(crate::acp::agent_storage::STORAGE_ROOT_ENV) {
        if let Some(candidate) = managed_node_bin_dir_from_data_root(Path::new(&storage_root)) {
            return Some(candidate);
        }
    }
    if let Some(data_root) = std::env::var_os("IYW_CLAW_DATA_DIR") {
        if let Some(candidate) = managed_node_bin_dir_from_data_root(Path::new(&data_root)) {
            return Some(candidate);
        }
    }
    let install_dir = executable.parent()?;
    let node_root = install_dir.parent()?.join("runtime").join("node");
    managed_node_bin_dir_from_node_root(&node_root)
}

pub(super) fn managed_node_bin_dir_from_data_root(data_root: &Path) -> Option<PathBuf> {
    managed_node_bin_dir_from_node_root(&data_root.join("runtime").join("node"))
}

fn managed_node_bin_dir_from_node_root(node_root: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(node_root.join("current.json")).ok()?;
    let state: ManagedNodeState = serde_json::from_str(&raw).ok()?;
    if !valid_version(&state.version) || state.platform != current_platform_dir() {
        return None;
    }

    let candidate = node_root.join(state.version).join(state.platform);
    let bin_dir = if cfg!(windows) {
        candidate.clone()
    } else {
        candidate.join("bin")
    };
    let canonical_root = std::fs::canonicalize(node_root).ok()?;
    let canonical_bin = std::fs::canonicalize(&bin_dir).ok()?;
    let (node, npm) = if cfg!(windows) {
        ("node.exe", "npm.cmd")
    } else {
        ("node", "npm")
    };
    if !canonical_bin.starts_with(canonical_root)
        || !canonical_bin.join(node).is_file()
        || !canonical_bin.join(npm).is_file()
    {
        return None;
    }
    Some(bin_dir)
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.split('.').count() == 3
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn current_platform_dir() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win-x64",
        ("windows", "aarch64") => "win-arm64",
        ("windows", "x86") => "win-x86",
        ("macos", "x86_64") => "darwin-x64",
        ("macos", "aarch64") => "darwin-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "unknown",
    }
}
