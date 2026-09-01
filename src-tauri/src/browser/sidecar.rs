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
            13_707_264,
            "5ffcad90cda06114730e8b202285c45ec0866d1b8d7876b561329e4a8cfbb126",
        ),
        ("macos", "x86_64") => (
            13_378_880,
            "d76cfc76885d5007f3c119008a80a145b381ec4dfdd202f43e46cd0829751774",
        ),
        ("macos", "aarch64") => (
            12_247_424,
            "e1e08f3b0a1c711750209e6a25b6f3a9dab7ed6e6a24b55a2556050b991fcc97",
        ),
        ("linux", "x86_64") => (
            14_021_032,
            "b699f24eebdb7fde91a34a9d697a1b84c3145f54327b60694b46f06b2972ce4d",
        ),
        ("linux", "aarch64") => (
            12_332_896,
            "1599fec4f4e75dc26fc08eecc06ca4b729a0361932b32a6afb99885f0f829ecb",
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
