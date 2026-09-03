use std::path::{Path, PathBuf};
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

#[cfg(not(target_os = "windows"))]
use tokio::process::Command;

use super::error::{BrowserError, BrowserErrorCode};
#[cfg(not(target_os = "windows"))]
use super::process::configure_hidden_process;
use super::types::{BrowserEngineKind, BrowserEngineSummary};

#[cfg(not(target_os = "windows"))]
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
    if let Some((path, version)) =
        crate::acp::version_center::managed_browser_engine_installation(data_root).await
    {
        return Ok(BrowserEngine {
            kind: BrowserEngineKind::Chromium,
            version,
            path,
            profile_source: None,
        });
    }
    #[cfg(target_os = "windows")]
    if let Some(engine) = super::engine_download::detect_cached_engine(data_root).await {
        return Ok(engine);
    }
    Err(managed_engine_not_found())
}

pub(super) async fn probe_engine(
    kind: BrowserEngineKind,
    path: PathBuf,
    profile_source: Option<PathBuf>,
) -> Option<BrowserEngine> {
    if !path.is_file() {
        return None;
    }
    #[cfg(target_os = "windows")]
    let version = windows_file_version(&path)?;
    #[cfg(not(target_os = "windows"))]
    let version = executable_version(&path).await?;
    Some(BrowserEngine {
        kind,
        version,
        path,
        profile_source,
    })
}

#[cfg(not(target_os = "windows"))]
async fn executable_version(path: &Path) -> Option<String> {
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
    Some(version.chars().take(128).collect())
}

#[cfg(target_os = "windows")]
fn windows_file_version(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut handle = 0_u32;
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0_u8; size as usize];
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let mut value = std::ptr::null_mut();
    let mut value_len = 0_u32;
    let root = ['\\' as u16, 0];
    if unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            root.as_ptr(),
            &mut value,
            &mut value_len,
        )
    } == 0
        || value_len < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
    {
        return None;
    }
    let info = unsafe { &*(value.cast::<VS_FIXEDFILEINFO>()) };
    Some(format!(
        "{}.{}.{}.{}",
        info.dwFileVersionMS >> 16,
        info.dwFileVersionMS & 0xffff,
        info.dwFileVersionLS >> 16,
        info.dwFileVersionLS & 0xffff
    ))
}

fn managed_engine_not_found() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserEngineNotFound,
        "No verified managed browser engine is installed",
    )
    .retryable(true)
}
