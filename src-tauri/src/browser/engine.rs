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

pub(super) async fn detect_engine(data_root: &Path) -> Result<BrowserEngine, BrowserError> {
    let path = crate::acp::version_center::managed_browser_engine_executable(data_root)
        .await
        .ok_or_else(managed_engine_not_found)?;
    probe_engine(BrowserEngineKind::Chromium, path, None)
        .await
        .ok_or_else(managed_engine_not_found)
}

pub(super) async fn probe_engine(
    kind: BrowserEngineKind,
    path: PathBuf,
    profile_source: Option<PathBuf>,
) -> Option<BrowserEngine> {
    if !path.is_file() {
        return None;
    }
    let version = probe_version(&path).await?;
    Some(BrowserEngine {
        kind,
        version: version.chars().take(128).collect(),
        path,
        profile_source,
    })
}

async fn probe_version(path: &Path) -> Option<String> {
    if executable_is_running(path) {
        return Some(installed_version(path));
    }
    let mut command = Command::new(path);
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
    raw.lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(128).collect())
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

fn managed_engine_not_found() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserEngineNotFound,
        "No verified managed browser engine is installed",
    )
    .retryable(true)
}
