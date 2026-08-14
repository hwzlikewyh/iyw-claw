use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::error::{BrowserError, BrowserErrorCode};
use super::types::{BrowserEngineKind, BrowserEngineSummary};

#[derive(Debug, Clone)]
pub(super) struct BrowserEngine {
    pub kind: BrowserEngineKind,
    pub path: PathBuf,
    pub version: String,
}

impl BrowserEngine {
    pub fn summary(&self) -> BrowserEngineSummary {
        BrowserEngineSummary {
            kind: self.kind,
            version: self.version.clone(),
        }
    }
}

pub(super) async fn detect_engine() -> Result<BrowserEngine, BrowserError> {
    #[cfg(target_os = "windows")]
    {
        for (kind, path) in windows_candidates() {
            if !path.is_file() {
                continue;
            }
            return Ok(BrowserEngine {
                kind,
                version: installed_version(&path),
                path,
            });
        }
    }
    Err(BrowserError::new(
        BrowserErrorCode::BrowserEngineNotFound,
        "Google Chrome or Microsoft Edge was not found",
    ))
}

fn installed_version(path: &Path) -> String {
    let Some(parent) = path.parent() else {
        return "unknown".to_string();
    };
    std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            parse_version(&name).map(|version| (version, name))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, name)| name)
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_version(value: &str) -> Option<(u64, u64, u64, u64)> {
    let mut parts = value.split('.').map(str::parse::<u64>);
    let version = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    parts.next().is_none().then_some(version)
}

#[cfg(target_os = "windows")]
fn windows_candidates() -> Vec<(BrowserEngineKind, PathBuf)> {
    let mut candidates = Vec::new();
    push_known_paths(&mut candidates);
    let mut seen = HashSet::new();
    candidates.retain(|(_, path)| seen.insert(path.to_string_lossy().to_ascii_lowercase()));
    candidates
}

#[cfg(target_os = "windows")]
fn push_known_paths(candidates: &mut Vec<(BrowserEngineKind, PathBuf)>) {
    let roots = [
        std::env::var_os("PROGRAMFILES"),
        std::env::var_os("PROGRAMFILES(X86)"),
    ];
    for root in roots.into_iter().flatten().map(PathBuf::from) {
        candidates.push((
            BrowserEngineKind::Chrome,
            root.join("Google/Chrome/Application/chrome.exe"),
        ));
        candidates.push((
            BrowserEngineKind::Edge,
            root.join("Microsoft/Edge/Application/msedge.exe"),
        ));
    }
    if let Some(root) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        candidates.push((
            BrowserEngineKind::Chrome,
            root.join("Google/Chrome/Application/chrome.exe"),
        ));
        candidates.push((
            BrowserEngineKind::Edge,
            root.join("Microsoft/Edge/Application/msedge.exe"),
        ));
    }
}
