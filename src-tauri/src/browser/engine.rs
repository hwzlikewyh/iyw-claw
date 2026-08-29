use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use super::error::{BrowserError, BrowserErrorCode};
use super::process::{configure_hidden_process, executable_is_running};
use super::types::{BrowserEngineKind, BrowserEngineSummary};

const ENGINE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub(super) struct BrowserEngine {
    pub kind: BrowserEngineKind,
    pub path: PathBuf,
    pub version: String,
    pub profile_source: Option<PathBuf>,
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
        for (kind, path, profile_source) in windows_candidates() {
            if let Some(engine) = probe_engine(kind, path, profile_source).await {
                return Ok(engine);
            }
        }
        let managed_path = super::engine_download::managed_engine_path();
        if let Some(engine) = probe_engine(BrowserEngineKind::Chromium, managed_path, None).await {
            return Ok(engine);
        }
    }
    Err(BrowserError::new(
        BrowserErrorCode::BrowserEngineNotFound,
        "No usable Chromium-compatible browser or managed Chromium engine was found",
    ))
}

pub(super) async fn probe_engine(
    kind: BrowserEngineKind,
    path: PathBuf,
    profile_source: Option<PathBuf>,
) -> Option<BrowserEngine> {
    if !path.is_file() {
        return None;
    }
    if !matches!(kind, BrowserEngineKind::Chromium) {
        return Some(BrowserEngine {
            kind,
            version: installed_version(&path),
            path,
            profile_source,
        });
    }
    if executable_is_running(&path) {
        return Some(BrowserEngine {
            kind,
            version: installed_version(&path),
            path,
            profile_source,
        });
    }
    let mut command = Command::new(&path);
    command.arg("--version");
    configure_hidden_process(&mut command);
    let output = tokio::time::timeout(ENGINE_PROBE_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr)
    } else {
        String::from_utf8_lossy(&output.stdout)
    };
    let version = raw
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| installed_version(&path));
    if version.is_empty() {
        return None;
    }
    Some(BrowserEngine {
        kind,
        version: version.chars().take(128).collect(),
        path,
        profile_source,
    })
}

fn installed_version(path: &Path) -> String {
    let Some(parent) = path.parent() else {
        return "installed".to_string();
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
        .unwrap_or_else(|| "installed".to_string())
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
fn windows_candidates() -> Vec<(BrowserEngineKind, PathBuf, Option<PathBuf>)> {
    let mut candidates = Vec::new();
    push_known_paths(&mut candidates);
    let mut seen = HashSet::new();
    candidates.retain(|(_, path, _)| seen.insert(path.to_string_lossy().to_ascii_lowercase()));
    candidates
}

#[cfg(target_os = "windows")]
fn push_known_paths(candidates: &mut Vec<(BrowserEngineKind, PathBuf, Option<PathBuf>)>) {
    let roots = [
        std::env::var_os("PROGRAMFILES"),
        std::env::var_os("PROGRAMFILES(X86)"),
    ];
    let kinds = [
        BrowserEngineKind::Chrome,
        BrowserEngineKind::Edge,
        BrowserEngineKind::Brave,
        BrowserEngineKind::Vivaldi,
        BrowserEngineKind::Opera,
    ];
    for kind in kinds {
        for root in roots.iter().flatten().map(PathBuf::from) {
            if let Some(relative) = installed_relative_path(kind) {
                push_candidate(candidates, kind, root.join(relative));
            }
        }
        if let Some(root) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            if let Some(relative) = installed_relative_path(kind) {
                push_candidate(candidates, kind, root.join(relative));
            }
        }
    }
    if let Some(root) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        push_candidate(
            candidates,
            BrowserEngineKind::Chromium,
            root.join("Chromium/Application/chrome.exe"),
        );
    }
}

#[cfg(target_os = "windows")]
fn installed_relative_path(kind: BrowserEngineKind) -> Option<&'static str> {
    match kind {
        BrowserEngineKind::Chrome => Some("Google/Chrome/Application/chrome.exe"),
        BrowserEngineKind::Edge => Some("Microsoft/Edge/Application/msedge.exe"),
        BrowserEngineKind::Brave => Some("BraveSoftware/Brave-Browser/Application/brave.exe"),
        BrowserEngineKind::Vivaldi => Some("Vivaldi/Application/vivaldi.exe"),
        BrowserEngineKind::Opera => Some("Opera/launcher.exe"),
        BrowserEngineKind::Chromium => None,
    }
}

#[cfg(target_os = "windows")]
fn push_candidate(
    candidates: &mut Vec<(BrowserEngineKind, PathBuf, Option<PathBuf>)>,
    kind: BrowserEngineKind,
    path: PathBuf,
) {
    let profile_source =
        user_data_path(kind).filter(|profile| profile.is_dir() && !executable_is_running(&path));
    candidates.push((kind, path, profile_source));
}

#[cfg(target_os = "windows")]
fn user_data_path(kind: BrowserEngineKind) -> Option<PathBuf> {
    match kind {
        BrowserEngineKind::Chrome => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Google/Chrome/User Data")),
        BrowserEngineKind::Edge => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Microsoft/Edge/User Data")),
        BrowserEngineKind::Brave => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("BraveSoftware/Brave-Browser/User Data")),
        BrowserEngineKind::Vivaldi => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Vivaldi/User Data")),
        BrowserEngineKind::Opera => std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Opera Software/Opera Stable")),
        BrowserEngineKind::Chromium => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Chromium/User Data")),
    }
}

pub(super) fn user_profile_source(
    kind: BrowserEngineKind,
    browser_executable: &Path,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        return user_data_path(kind)
            .filter(|profile| profile.is_dir() && !executable_is_running(browser_executable));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (kind, browser_executable);
        None
    }
}
