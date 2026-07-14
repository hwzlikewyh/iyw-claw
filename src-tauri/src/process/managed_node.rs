use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedNodeState {
    version: String,
    platform: String,
}

pub(super) fn managed_node_bin_dir(executable: &Path) -> Option<PathBuf> {
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
    if !valid_version(&state.version) || !matches!(state.platform.as_str(), "win-x64" | "win-arm64")
    {
        return None;
    }

    let candidate = node_root.join(state.version).join(state.platform);
    let canonical_root = std::fs::canonicalize(node_root).ok()?;
    let canonical_candidate = std::fs::canonicalize(&candidate).ok()?;
    if !canonical_candidate.starts_with(canonical_root)
        || !canonical_candidate.join("node.exe").is_file()
        || !canonical_candidate.join("npm.cmd").is_file()
    {
        return None;
    }
    Some(candidate)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{managed_node_bin_dir, managed_node_bin_dir_from_data_root};

    #[test]
    fn resolves_verified_node_runtime_below_the_data_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime/node/24.0.0/win-x64");
        fs::create_dir_all(&runtime).expect("runtime dir");
        fs::write(runtime.join("node.exe"), b"node").expect("node");
        fs::write(runtime.join("npm.cmd"), b"npm").expect("npm");
        fs::write(
            temp.path().join("runtime/node/current.json"),
            r#"{"version":"24.0.0","platform":"win-x64"}"#,
        )
        .expect("current state");

        assert_eq!(
            managed_node_bin_dir_from_data_root(temp.path()),
            Some(runtime)
        );
    }

    #[test]
    fn resolves_verified_node_runtime_beside_install_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let install_dir = temp.path().join("iyw-claw");
        let runtime = temp.path().join("runtime/node/24.0.0/win-x64");
        fs::create_dir_all(&runtime).expect("runtime dir");
        fs::write(runtime.join("node.exe"), b"node").expect("node");
        fs::write(runtime.join("npm.cmd"), b"npm").expect("npm");
        fs::write(
            temp.path().join("runtime/node/current.json"),
            r#"{"version":"24.0.0","platform":"win-x64","path":"C:/ignored"}"#,
        )
        .expect("current state");

        let resolved = managed_node_bin_dir(&install_dir.join("iyw-claw.exe"));

        assert_eq!(resolved, Some(runtime));
    }

    #[test]
    fn rejects_escaping_or_incomplete_managed_node_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let install_dir = temp.path().join("iyw-claw");
        fs::create_dir_all(temp.path().join("runtime/node")).expect("node root");
        let state = temp.path().join("runtime/node/current.json");
        fs::write(&state, r#"{"version":"..","platform":"win-x64"}"#).expect("escaping state");
        assert_eq!(
            managed_node_bin_dir(&install_dir.join("iyw-claw.exe")),
            None
        );

        fs::write(&state, r#"{"version":"24.0.0","platform":"win-x64"}"#)
            .expect("incomplete state");
        fs::create_dir_all(temp.path().join("runtime/node/24.0.0/win-x64")).expect("runtime dir");
        fs::write(
            temp.path().join("runtime/node/24.0.0/win-x64/node.exe"),
            b"node",
        )
        .expect("node");
        assert_eq!(
            managed_node_bin_dir(&install_dir.join("iyw-claw.exe")),
            None
        );
    }
}
