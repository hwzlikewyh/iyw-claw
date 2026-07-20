use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedGitState {
    version: String,
    platform: String,
}

pub(super) fn managed_git_bin_dir(data_root: &Path) -> Option<PathBuf> {
    let git_root = data_root.join("runtime").join("git");
    let raw = std::fs::read_to_string(git_root.join("current.json")).ok()?;
    let state: ManagedGitState = serde_json::from_str(&raw).ok()?;
    if !valid_version(&state.version)
        || !matches!(state.platform.as_str(), "win-x64" | "win-arm64" | "win-x86")
    {
        return None;
    }

    let candidate = git_root
        .join(state.version)
        .join(state.platform)
        .join("cmd");
    let canonical_root = std::fs::canonicalize(&git_root).ok()?;
    let canonical_candidate = std::fs::canonicalize(&candidate).ok()?;
    if !canonical_candidate.starts_with(canonical_root)
        || !canonical_candidate.join("git.exe").is_file()
    {
        return None;
    }
    Some(candidate)
}

fn valid_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    (3..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}
