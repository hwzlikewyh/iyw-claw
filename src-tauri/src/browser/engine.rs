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
    let (path, version) =
        crate::acp::version_center::managed_browser_engine_installation(data_root)
            .await
            .ok_or_else(managed_engine_not_found)?;
    Ok(BrowserEngine {
        kind: BrowserEngineKind::Chromium,
        version,
        path,
        profile_source: None,
    })
}

pub(super) async fn probe_engine(
    kind: BrowserEngineKind,
    path: PathBuf,
    profile_source: Option<PathBuf>,
) -> Option<BrowserEngine> {
    if !path.is_file() {
        return None;
    }
    if executable_is_running(&path) {
        return Some(BrowserEngine {
            kind,
            version: "running".to_string(),
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
        .map(ToString::to_string)?;
    Some(BrowserEngine {
        kind,
        version: version.chars().take(128).collect(),
        path,
        profile_source,
    })
}

fn managed_engine_not_found() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserEngineNotFound,
        "No verified managed browser engine is installed",
    )
    .retryable(true)
}
