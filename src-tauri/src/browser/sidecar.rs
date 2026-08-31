use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::error::{BrowserError, BrowserErrorCode};
use super::process::configure_hidden_process;
use super::types::BROWSER_SIDECAR_VERSION;

pub const AGENT_BROWSER_VERSION: &str = BROWSER_SIDECAR_VERSION;
pub const AGENT_BROWSER_SIZE: u64 = 13_665_280;
pub const AGENT_BROWSER_SHA256: &str =
    "def2614c2c193518463ad9126718a1ff828a7bf217d7f75f156249c0dbb16c83";
const VERIFY_TIMEOUT: Duration = Duration::from_secs(5);

pub fn sidecar_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(override_path) = std::env::var_os("IYW_CLAW_AGENT_BROWSER_PATH") {
        candidates.push(PathBuf::from(override_path));
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            candidates.push(dir.join(sidecar_filename("agent-browser")));
            candidates.push(dir.join(sidecar_filename(&format!(
                "agent-browser-{AGENT_BROWSER_VERSION}"
            ))));
        }
    }
    if cfg!(debug_assertions) {
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "binaries/agent-browser-x86_64-pc-windows-msvc{}",
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
    verify_size_and_hash(&path)?;
    verify_version(&path).await?;
    Ok(path)
}

fn verify_size_and_hash(path: &Path) -> Result<(), BrowserError> {
    let metadata = std::fs::metadata(path).map_err(|_| integrity_error())?;
    if metadata.len() != AGENT_BROWSER_SIZE {
        return Err(integrity_error());
    }
    let mut file = std::fs::File::open(path).map_err(|_| integrity_error())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| integrity_error())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    (format!("{:x}", hasher.finalize()) == AGENT_BROWSER_SHA256)
        .then_some(())
        .ok_or_else(integrity_error)
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

fn sidecar_filename(base: &str) -> String {
    format!("{base}{}", std::env::consts::EXE_SUFFIX)
}

fn integrity_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserSidecarIntegrityFailed,
        "The bundled browser controller failed its integrity check",
    )
}
