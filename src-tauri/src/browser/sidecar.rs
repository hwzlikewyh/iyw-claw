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
            13_837_312,
            "412ff72737a109e93f5304b0ff76c988fb6f1f451d0fc7e010577922bcc20ff3",
        ),
        ("macos", "x86_64") => (
            13_510_280,
            "45d9ac061a7d72e61eaff905326e2e19365f4dadb12142ea2f2d76d84689c708",
        ),
        ("macos", "aarch64") => (
            12_363_200,
            "b2106ab39db0838e7b1772f7f26f760518de56d09053150c56f9dddf15af997d",
        ),
        ("linux", "x86_64") => (
            14_156_776,
            "56d15181e51e00213f907fcf39707cfc76bfa804ff20f5a9373661c73f96de5e",
        ),
        ("linux", "aarch64") => (
            12_442_720,
            "aeb556addca3903601a433de1acad3ace1c9c61d170084bf58d875884599a990",
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
