use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use super::error::{BrowserError, BrowserErrorCode};
use super::process::configure_hidden_process;
use super::types::BROWSER_SIDECAR_VERSION;

pub const AGENT_BROWSER_VERSION: &str = BROWSER_SIDECAR_VERSION;
const VERIFY_TIMEOUT: Duration = Duration::from_secs(5);

pub fn sidecar_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(override_path) = std::env::var_os("IYW_CLAW_AGENT_BROWSER_PATH") {
        candidates.push(PathBuf::from(override_path));
    }
    if let Some(managed) = crate::managed_environment::entrypoint("agent-browser", "agent-browser")
    {
        candidates.push(managed);
    }
    if cfg!(debug_assertions) {
        if let Some(override_path) = std::env::var_os("IYW_CLAW_AGENT_BROWSER_PATH") {
            candidates.push(PathBuf::from(override_path));
        }
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "binaries/agent-browser-{}{}",
            env!("IYW_CLAW_TARGET_TRIPLE"),
            std::env::consts::EXE_SUFFIX
        )));
    }
    candidates.dedup();
    candidates
}

pub async fn verify_sidecar() -> Result<PathBuf, BrowserError> {
    let path = sidecar_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            BrowserError::new(
                BrowserErrorCode::BrowserSidecarMissing,
                "The bundled browser controller is missing",
            )
        })?;
    verify_version(&path).await?;
    Ok(path)
}

async fn verify_version(path: &Path) -> Result<(), BrowserError> {
    let mut command = Command::new(path);
    command.arg("--version");
    configure_hidden_process(&mut command);
    let output = tokio::time::timeout(VERIFY_TIMEOUT, command.output())
        .await
        .map_err(|_| integrity_error())?
        .map_err(|_| integrity_error())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !stdout.trim().ends_with(AGENT_BROWSER_VERSION) {
        return Err(integrity_error());
    }
    Ok(())
}

fn integrity_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserSidecarIntegrityFailed,
        "The bundled browser controller failed its integrity check",
    )
}
