use std::path::{Path, PathBuf};

use super::error::{BrowserError, BrowserErrorCode};
use super::types::{BrowserEngineKind, BrowserEngineSummary};

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

fn managed_engine_not_found() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserEngineNotFound,
        "No verified managed browser engine is installed",
    )
    .retryable(true)
}
