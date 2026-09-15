use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
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
    verify_size_and_hash(&path)?;
    verify_version(&path).await?;
    Ok(path)
}

fn verify_size_and_hash(path: &Path) -> Result<(), BrowserError> {
    let (expected_size, expected_hash) = expected_asset_digest();
    let metadata = std::fs::metadata(path).map_err(|_| integrity_error())?;
    if metadata.len() != expected_size {
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
    (format!("{:x}", hasher.finalize()) == expected_hash)
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

fn expected_asset_digest() -> (u64, &'static str) {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => (
            13_942_272,
            "29a003139ff4eb96fa4d1ed341830b26eb3e082843bf776b4e88ad3443bb8fde",
        ),
        ("macos", "x86_64") => (
            13_592_392,
            "c79d1e0525c0bf79df9eec355269ae40bcda9c4a3fce3f242c24faecaaaeef84",
        ),
        ("macos", "aarch64") => (
            12_429_376,
            "e52f06476ea0f1d14357c1924ce1d7f1bf08279f2642d74ccfa7ee935c46aea1",
        ),
        ("linux", "x86_64") => (
            14_253_840,
            "f8e5f9294bd0da70dda61854f12004fd61c668cd682bfb600cdf6d0df73dea69",
        ),
        ("linux", "aarch64") => (
            12_507_648,
            "d54d3e1262dc1aa0906e0677adc6d0cbb40d1274631f4cf77136bf23a0bc20e9",
        ),
        _ => (0, ""),
    }
}

fn integrity_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserSidecarIntegrityFailed,
        "The bundled browser controller failed its integrity check",
    )
}
