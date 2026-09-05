use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use super::error::{BrowserError, BrowserErrorCode};
use super::process::configure_hidden_process;
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
    if let Some((path, marker_version)) =
        crate::acp::version_center::managed_browser_engine_installation(data_root).await
    {
        if let Some(engine) = probe_engine(BrowserEngineKind::Chromium, path.clone(), None).await {
            return Ok(engine);
        }
        tracing::warn!(
            target: "iyw_claw_browser",
            path = %path.display(),
            marker_version = %marker_version,
            "managed browser engine failed its startup probe; trying fallback engines"
        );
    }
    #[cfg(target_os = "windows")]
    if let Some(engine) = probe_engine(
        BrowserEngineKind::Chromium,
        super::engine_download::managed_engine_path(data_root),
        None,
    )
    .await
    {
        return Ok(engine);
    }
    for (kind, path) in system_engine_candidates() {
        if let Some(engine) = probe_engine(kind, path, None).await {
            return Ok(engine);
        }
    }
    Err(managed_engine_not_found())
}

fn system_engine_candidates() -> Vec<(BrowserEngineKind, PathBuf)> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "windows")]
    push_windows_candidates(&mut candidates);
    #[cfg(target_os = "macos")]
    push_macos_candidates(&mut candidates);
    #[cfg(target_os = "linux")]
    push_linux_candidates(&mut candidates);
    let mut seen = HashSet::new();
    candidates.retain(|(_, path)| seen.insert(path.to_string_lossy().to_ascii_lowercase()));
    candidates
}

#[cfg(target_os = "windows")]
fn push_windows_candidates(candidates: &mut Vec<(BrowserEngineKind, PathBuf)>) {
    let roots = [
        std::env::var_os("PROGRAMFILES"),
        std::env::var_os("PROGRAMFILES(X86)"),
        std::env::var_os("LOCALAPPDATA"),
    ];
    let browsers = [
        (
            BrowserEngineKind::Chrome,
            "Google/Chrome/Application/chrome.exe",
        ),
        (
            BrowserEngineKind::Edge,
            "Microsoft/Edge/Application/msedge.exe",
        ),
        (
            BrowserEngineKind::Brave,
            "BraveSoftware/Brave-Browser/Application/brave.exe",
        ),
        (
            BrowserEngineKind::Vivaldi,
            "Vivaldi/Application/vivaldi.exe",
        ),
        (BrowserEngineKind::Opera, "Opera/launcher.exe"),
        (
            BrowserEngineKind::Chromium,
            "Chromium/Application/chrome.exe",
        ),
    ];
    for root in roots.into_iter().flatten().map(PathBuf::from) {
        for (kind, relative) in browsers {
            candidates.push((kind, root.join(relative)));
        }
    }
    for root in std::env::var_os("PATH")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
    {
        for (kind, relative) in browsers {
            candidates.push((kind, root.join(relative)));
        }
    }
}

#[cfg(target_os = "macos")]
fn push_macos_candidates(candidates: &mut Vec<(BrowserEngineKind, PathBuf)>) {
    let roots = [
        Some(PathBuf::from("/Applications")),
        dirs::home_dir().map(|home| home.join("Applications")),
    ];
    let browsers = [
        (
            BrowserEngineKind::Chrome,
            "Google Chrome.app/Contents/MacOS/Google Chrome",
        ),
        (
            BrowserEngineKind::Edge,
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ),
        (
            BrowserEngineKind::Brave,
            "Brave Browser.app/Contents/MacOS/Brave Browser",
        ),
        (
            BrowserEngineKind::Vivaldi,
            "Vivaldi.app/Contents/MacOS/Vivaldi",
        ),
        (BrowserEngineKind::Opera, "Opera.app/Contents/MacOS/Opera"),
        (
            BrowserEngineKind::Chromium,
            "Chromium.app/Contents/MacOS/Chromium",
        ),
    ];
    for root in roots.into_iter().flatten() {
        for (kind, relative) in browsers {
            candidates.push((kind, root.join(relative)));
        }
    }
}

#[cfg(target_os = "linux")]
fn push_linux_candidates(candidates: &mut Vec<(BrowserEngineKind, PathBuf)>) {
    let browsers = [
        (BrowserEngineKind::Chrome, "google-chrome"),
        (BrowserEngineKind::Chrome, "google-chrome-stable"),
        (BrowserEngineKind::Edge, "microsoft-edge"),
        (BrowserEngineKind::Edge, "microsoft-edge-stable"),
        (BrowserEngineKind::Brave, "brave-browser"),
        (BrowserEngineKind::Vivaldi, "vivaldi"),
        (BrowserEngineKind::Opera, "opera"),
        (BrowserEngineKind::Chromium, "chromium"),
        (BrowserEngineKind::Chromium, "chromium-browser"),
    ];
    let mut directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    directories.extend([
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/snap/bin"),
    ]);
    for directory in directories {
        for (kind, name) in browsers {
            candidates.push((kind, directory.join(name)));
        }
    }
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
    #[cfg(target_os = "windows")]
    if !process_starts(&path).await {
        return None;
    }
    Some(BrowserEngine {
        kind,
        version,
        path,
        profile_source,
    })
}

#[cfg(target_os = "windows")]
async fn process_starts(path: &Path) -> bool {
    let mut command = Command::new(path);
    command.arg("--version");
    configure_hidden_process(&mut command);
    tokio::time::timeout(ENGINE_PROBE_TIMEOUT, command.output())
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|output| output.status.success())
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
