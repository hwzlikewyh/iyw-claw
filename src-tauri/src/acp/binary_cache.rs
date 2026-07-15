use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::acp::registry;
use crate::models::agent::AgentType;

/// Process-local counter appended to rename-aside trash directory names. Guards
/// against the rare case where two `clear_agent_cache` calls land in the same
/// `SystemTime::now()` tick (Windows `GetSystemTimePreciseAsFileTime` has ~100ns
/// resolution) and would otherwise collide on the rename target.
static TRASH_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Pinned `uv` toolchain version iyw-claw downloads for Python ACP agents.
const UV_TOOL_VERSION: &str = "0.8.10";

/// Locate an iyw-claw-managed uv tool binary (`uv` or `uvx`) below the private
/// Agent storage root. Missing or incompatible files are never resolved from
/// the process PATH by this module.
pub fn managed_uv_tool_path(paths: &AgentStoragePaths, tool: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    };
    uv_tool_dir_for(paths).join(exe)
}

pub fn find_cached_uv_tool(paths: &AgentStoragePaths, tool: &str) -> Option<PathBuf> {
    let path = managed_uv_tool_path(paths, tool);
    (path.is_file() && is_binary_file_compatible(&path)).then_some(path)
}

pub fn bundled_uv_tool_paths(executable: &Path) -> Option<(PathBuf, PathBuf)> {
    let directory = executable.parent()?;
    let (uv, uvx) = if cfg!(windows) {
        ("uv.exe", "uvx.exe")
    } else {
        ("uv", "uvx")
    };
    Some((directory.join(uv), directory.join(uvx)))
}

pub fn seed_bundled_uv_tools(
    paths: &AgentStoragePaths,
    executable: &Path,
) -> Result<bool, AcpError> {
    let Some((bundled_uv, bundled_uvx)) = bundled_uv_tool_paths(executable) else {
        return Ok(false);
    };
    if !bundled_uv.is_file() || !bundled_uvx.is_file() {
        return Ok(false);
    }
    if !is_binary_file_compatible(&bundled_uv) || !is_binary_file_compatible(&bundled_uvx) {
        return Ok(false);
    }
    if find_cached_uv_tool(paths, "uv").is_some() && find_cached_uv_tool(paths, "uvx").is_some() {
        return Ok(false);
    }
    let staging = paths
        .staging_dir()
        .join(format!("uv-bundled-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging)
        .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
    let names = if cfg!(windows) {
        [(bundled_uv, "uv.exe"), (bundled_uvx, "uvx.exe")]
    } else {
        [(bundled_uv, "uv"), (bundled_uvx, "uvx")]
    };
    for (source, name) in names {
        let target = staging.join(name);
        std::fs::copy(source, &target)
            .map_err(|error| AcpError::DownloadFailed(error.to_string()))?;
        set_executable_permissions(&target)?;
    }
    activate_staged_directory(paths, &staging, &uv_tool_dir_for(paths), "uv-runtime")?;
    Ok(true)
}

pub fn uv_tool_dir_for(paths: &AgentStoragePaths) -> PathBuf {
    paths
        .uv_runtime_dir()
        .join(UV_TOOL_VERSION)
        .join(registry::current_platform())
}

pub fn uv_runtime_env(paths: &AgentStoragePaths) -> BTreeMap<&'static str, PathBuf> {
    BTreeMap::from([
        ("UV_CACHE_DIR", paths.uv_cache_dir()),
        ("UV_TOOL_DIR", paths.uv_runtime_dir().join("tools")),
        ("UV_TOOL_BIN_DIR", paths.uv_runtime_dir().join("bin")),
    ])
}

/// Build the astral-sh/uv release archive URL for the current platform.
fn uv_archive_url() -> Option<String> {
    let (target, ext) = match registry::current_platform() {
        "darwin-aarch64" => ("aarch64-apple-darwin", "tar.gz"),
        "darwin-x86_64" => ("x86_64-apple-darwin", "tar.gz"),
        "linux-aarch64" => ("aarch64-unknown-linux-gnu", "tar.gz"),
        "linux-x86_64" => ("x86_64-unknown-linux-gnu", "tar.gz"),
        "windows-aarch64" => ("aarch64-pc-windows-msvc", "zip"),
        "windows-i686" => ("i686-pc-windows-msvc", "zip"),
        "windows-x86_64" => ("x86_64-pc-windows-msvc", "zip"),
        _ => return None,
    };
    Some(format!(
        "https://github.com/astral-sh/uv/releases/download/{UV_TOOL_VERSION}/uv-{target}.{ext}"
    ))
}

/// Download + cache the `uv` toolchain (`uv` + `uvx`) below private Agent storage.
/// Idempotent: returns the cached `uvx` path immediately if already present.
pub async fn ensure_uv_tool(
    paths: &AgentStoragePaths,
    on_progress: impl Fn(&str),
) -> Result<PathBuf, AcpError> {
    if let Some(uvx) = find_cached_uv_tool(paths, "uvx") {
        on_progress("uv already cached, skipping download");
        return Ok(uvx);
    }

    let url = uv_archive_url().ok_or_else(|| {
        AcpError::PlatformNotSupported(format!(
            "uv is not available for platform {}",
            registry::current_platform()
        ))
    })?;

    let operation_id = uuid::Uuid::new_v4();
    let staging_dir = paths.staging_dir().join(format!("uv-{operation_id}"));
    let archive_path = paths
        .downloads_dir()
        .join(format!("uv-{UV_TOOL_VERSION}-{operation_id}.archive"));
    std::fs::create_dir_all(paths.downloads_dir())
        .map_err(|e| AcpError::DownloadFailed(format!("failed to create downloads dir: {e}")))?;
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to create uv staging dir: {e}")))?;

    let (uv_name, uvx_name) = if cfg!(windows) {
        ("uv.exe", "uvx.exe")
    } else {
        ("uv", "uvx")
    };

    let result: Result<PathBuf, AcpError> = async {
        on_progress(&format!("Downloading uv {UV_TOOL_VERSION}..."));
        download_file_with_progress(&url, &archive_path, &on_progress).await?;

        let extract_dir = staging_dir.join("extracted");
        std::fs::create_dir_all(&extract_dir)
            .map_err(|e| AcpError::DownloadFailed(format!("failed to create extract dir: {e}")))?;

        on_progress("Extracting uv...");
        if url.ends_with(".tar.gz") {
            extract_tar_gz(&archive_path, &extract_dir)?;
        } else if url.ends_with(".zip") {
            extract_zip(&archive_path, &extract_dir)?;
        } else {
            return Err(AcpError::DownloadFailed(format!(
                "unsupported uv archive format: {url}"
            )));
        }

        // The uv archive ships both `uv` and `uvx`; cache both so the resolver
        // and any direct `uv` invocation find them.
        for name in [uv_name, uvx_name] {
            let extracted = find_binary_recursive(&extract_dir, name).ok_or_else(|| {
                AcpError::DownloadFailed(format!("'{name}' not found in uv archive"))
            })?;
            let staged_path = staging_dir.join(name);
            std::fs::copy(&extracted, &staged_path)
                .map_err(|e| AcpError::DownloadFailed(format!("failed to copy {name}: {e}")))?;
            if !is_binary_file_compatible(&staged_path) {
                return Err(AcpError::DownloadFailed(format!(
                    "downloaded {name} format is invalid for current platform"
                )));
            }
            set_executable_permissions(&staged_path)?;
        }
        std::fs::remove_dir_all(&extract_dir).map_err(|e| {
            AcpError::DownloadFailed(format!("failed to finalize uv staging dir: {e}"))
        })?;
        activate_staged_directory(paths, &staging_dir, &uv_tool_dir_for(paths), "uv-runtime")?;
        on_progress("uv installed successfully");
        find_cached_uv_tool(paths, "uvx")
            .ok_or_else(|| AcpError::DownloadFailed("uvx missing after install".into()))
    }
    .await;

    let _ = std::fs::remove_file(&archive_path);
    let _ = std::fs::remove_dir_all(&staging_dir);
    result
}

/// Marker recording that a `Uvx` agent's package has been pre-fetched into
/// uvx's cache (written by the prepare step). The file content is the prepared
/// version string. Lets the connect/status paths report readiness without
/// introspecting uvx's internal cache or triggering a download.
fn uvx_prepared_marker_for(paths: &AgentStoragePaths, registry_id: &str) -> PathBuf {
    paths.uv_runtime_dir().join("prepared").join(registry_id)
}

/// Return the prepared version for a Uvx agent, or `None` if it has not been
/// prepared yet.
pub fn uvx_prepared_version(paths: &AgentStoragePaths, agent_type: AgentType) -> Option<String> {
    let path = uvx_prepared_marker_for(paths, registry::registry_id_for(agent_type));
    let raw = std::fs::read_to_string(path).ok()?;
    let v = raw.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Record that a Uvx agent's package (at `version`) has been pre-fetched.
pub fn mark_uvx_agent_prepared(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AcpError> {
    let path = uvx_prepared_marker_for(paths, registry::registry_id_for(agent_type));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AcpError::DownloadFailed(format!("create uvx marker dir failed: {e}")))?;
    }
    std::fs::write(&path, version.as_bytes())
        .map_err(|e| AcpError::DownloadFailed(format!("write uvx marker failed: {e}")))
}

/// Remove a Uvx agent's prepared marker (used on uninstall). Absent marker is OK.
pub fn clear_uvx_agent_prepared(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
) -> Result<(), AcpError> {
    let path = uvx_prepared_marker_for(paths, registry::registry_id_for(agent_type));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AcpError::DownloadFailed(format!(
            "remove uvx marker failed: {e}"
        ))),
    }
}

fn normalize_version_label(version: &str) -> String {
    let trimmed = version.trim();
    if let Some(stripped) = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
    {
        stripped.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn agent_cache_key(agent_type: AgentType) -> String {
    registry::registry_id_for(agent_type).to_string()
}

fn binary_dir_for(
    paths: &AgentStoragePaths,
    agent_id: &str,
    version: &str,
) -> Result<PathBuf, AcpError> {
    binary_dir_from_root(&paths.binary_runtime_dir(), agent_id, version)
}

fn binary_dir_from_root(root: &Path, agent_id: &str, version: &str) -> Result<PathBuf, AcpError> {
    let version = normalize_version_label(version);
    if version.is_empty() {
        return Err(AcpError::DownloadFailed(
            "binary version is empty".to_string(),
        ));
    }

    Ok(root
        .join(agent_id)
        .join(version)
        .join(registry::current_platform()))
}

fn binary_trash_dir(paths: &AgentStoragePaths) -> PathBuf {
    paths.trash_dir().join("binary")
}

fn trash_entry_path(paths: &AgentStoragePaths, category: &str, label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = TRASH_COUNTER.fetch_add(1, Ordering::Relaxed);
    paths
        .trash_dir()
        .join(category)
        .join(format!("{label}-{stamp}-{counter}"))
}

fn activate_staged_directory(
    paths: &AgentStoragePaths,
    staging_dir: &Path,
    final_dir: &Path,
    trash_label: &str,
) -> Result<(), AcpError> {
    let parent = final_dir.parent().ok_or_else(|| {
        AcpError::DownloadFailed("runtime destination has no parent directory".into())
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AcpError::DownloadFailed(format!("create runtime dir failed: {e}")))?;

    let previous = if final_dir.exists() {
        let aside = trash_entry_path(paths, "runtime", trash_label);
        if let Some(parent) = aside.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AcpError::DownloadFailed(format!("create runtime trash dir failed: {e}"))
            })?;
        }
        std::fs::rename(final_dir, &aside).map_err(|e| {
            AcpError::DownloadFailed(format!("move previous runtime aside failed: {e}"))
        })?;
        Some(aside)
    } else {
        None
    };

    if let Err(error) = std::fs::rename(staging_dir, final_dir) {
        if let Some(previous) = previous.as_ref() {
            let _ = std::fs::rename(previous, final_dir);
        }
        return Err(AcpError::DownloadFailed(format!(
            "activate staged runtime failed: {error}"
        )));
    }

    if let Some(previous) = previous {
        let _ = std::fs::remove_dir_all(previous);
    }
    Ok(())
}

fn activate_staged_binary(
    paths: &AgentStoragePaths,
    agent_id: &str,
    version: &str,
    executable_name: &str,
    staging_dir: &Path,
) -> Result<PathBuf, AcpError> {
    let staged_binary = staging_dir.join(executable_name);
    if !is_binary_file_compatible(&staged_binary) {
        let _ = std::fs::remove_dir_all(staging_dir);
        return Err(AcpError::DownloadFailed(
            "downloaded binary format is invalid for current platform".into(),
        ));
    }
    if let Err(error) = set_executable_permissions(&staged_binary) {
        let _ = std::fs::remove_dir_all(staging_dir);
        return Err(error);
    }

    let final_dir = match binary_dir_for(paths, agent_id, version) {
        Ok(dir) => dir,
        Err(error) => {
            let _ = std::fs::remove_dir_all(staging_dir);
            return Err(error);
        }
    };
    if let Err(error) = activate_staged_directory(paths, staging_dir, &final_dir, agent_id) {
        let _ = std::fs::remove_dir_all(staging_dir);
        return Err(error);
    }
    Ok(final_dir.join(executable_name))
}

pub fn clear_agent_cache(paths: &AgentStoragePaths, agent_type: AgentType) -> Result<(), AcpError> {
    let agent_id = agent_cache_key(agent_type);
    let dir = paths.binary_runtime_dir().join(&agent_id);
    if !dir.exists() {
        return Ok(());
    }

    if std::fs::remove_dir_all(&dir).is_ok() {
        return Ok(());
    }

    // Windows: a running `<cmd>.exe` (ours or anti-virus scanning it) keeps the
    // file locked, so `remove_dir_all` returns ERROR_ACCESS_DENIED. NTFS allows
    // renaming a directory whose children are locked because rename only
    // updates the parent directory entry; the locked file's FILE_OBJECT keeps
    // working under the new path. The aside is swept on next startup.
    let trash_root = binary_trash_dir(paths);
    let _ = std::fs::create_dir_all(&trash_root);
    let aside = trash_entry_path(paths, "binary", &agent_id);
    std::fs::rename(&dir, &aside)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to clear cache: {e}")))?;

    let _ = std::fs::remove_dir_all(&aside);
    Ok(())
}

/// Best-effort cleanup of trash directories left behind by
/// `clear_agent_cache`'s rename-aside fallback. Designed to be run from a
/// detached OS thread at startup: every error path is silently swallowed,
/// no logs, no panics escape, no subprocesses spawned. Whatever cannot be
/// removed (e.g. a binary still locked by an external process) is left for
/// the next startup.
///
/// Iterates children rather than nuking the parent so that a concurrent
/// `clear_agent_cache` racing to rename a fresh entry into `.trash/` cannot
/// have its target directory yanked out from under it.
pub fn sweep_trash(paths: &AgentStoragePaths) {
    for trash in [binary_trash_dir(paths), paths.trash_dir().join("runtime")] {
        let Ok(entries) = std::fs::read_dir(&trash) else {
            continue;
        };
        for entry in entries.flatten() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn installed_binary_path(
    paths: &AgentStoragePaths,
    agent_id: &str,
    version: &str,
    cmd_name: &str,
) -> Option<PathBuf> {
    let bin_name = if cfg!(target_os = "windows") {
        format!("{cmd_name}.exe")
    } else {
        cmd_name.to_string()
    };

    let normalized = normalize_version_label(version);
    if normalized.is_empty() {
        return None;
    }

    let path = binary_dir_for(paths, agent_id, &normalized)
        .ok()?
        .join(bin_name);

    if !path.exists() {
        return None;
    }
    if is_binary_file_compatible(path.as_path()) {
        return Some(path);
    }
    let _ = std::fs::remove_file(path);
    None
}

fn installed_version_labels(
    paths: &AgentStoragePaths,
    agent_id: &str,
    cmd_name: &str,
) -> Result<Vec<String>, AcpError> {
    let root = paths.binary_runtime_dir().join(agent_id);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();
    let mut seen = HashSet::new();
    let entries = std::fs::read_dir(&root)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to read cache dir: {e}")))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let raw_version = entry.file_name().to_string_lossy().to_string();
        let normalized = normalize_version_label(&raw_version);
        if normalized.is_empty() {
            continue;
        }

        if installed_binary_path(paths, agent_id, &normalized, cmd_name).is_some()
            && seen.insert(normalized.clone())
        {
            versions.push(normalized);
        }
    }

    Ok(versions)
}

fn installed_version_for_agent(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    cmd_name: &str,
) -> Result<Option<String>, AcpError> {
    let agent_id = agent_cache_key(agent_type);
    let mut versions = installed_version_labels(paths, &agent_id, cmd_name)?;
    if versions.is_empty() {
        return Ok(None);
    }
    versions.sort_by(|a, b| version_cmp(a, b));
    Ok(versions.pop())
}

pub fn detect_installed_version(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    cmd_name: &str,
) -> Result<Option<String>, AcpError> {
    installed_version_for_agent(paths, agent_type, cmd_name)
}

/// Return the best cached binary across all installed versions.
///
/// This returns the path + version label of the highest semver-ish
/// version cached on disk, regardless of what the registry considers
/// the "recommended" version. The session-page connect path uses this
/// to tolerate older-but-still-usable cached binaries (e.g. the user
/// hasn't upgraded yet) — the Settings page will continue to surface
/// an "upgrade available" hint via the separate version-badge path.
///
/// Returns Ok(None) when no usable binary is cached.
pub fn find_best_cached_binary_for_agent(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    cmd_name: &str,
) -> Result<Option<(PathBuf, String)>, AcpError> {
    let agent_id = agent_cache_key(agent_type);
    let mut versions = installed_version_labels(paths, &agent_id, cmd_name)?;
    if versions.is_empty() {
        return Ok(None);
    }
    versions.sort_by(|a, b| version_cmp(a, b));
    while let Some(version) = versions.pop() {
        if let Some(path) = installed_binary_path(paths, &agent_id, &version, cmd_name) {
            return Ok(Some((path, version)));
        }
    }
    Ok(None)
}

fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_parts = parse_version_parts(a);
    let mut b_parts = parse_version_parts(b);
    let len = a_parts.len().max(b_parts.len());
    a_parts.resize(len, 0);
    b_parts.resize(len, 0);

    for i in 0..len {
        match a_parts[i].cmp(&b_parts[i]) {
            std::cmp::Ordering::Equal => continue,
            order => return order,
        }
    }
    a.cmp(b)
}

fn parse_version_parts(input: &str) -> Vec<u32> {
    input
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split('.')
        .map(|part| {
            let numeric: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            numeric.parse::<u32>().unwrap_or(0)
        })
        .collect()
}

/// Same as `ensure_binary_for_agent` but calls `on_progress` with human-readable
/// status messages during download / extraction.
pub async fn ensure_binary_for_agent_with_progress(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    archive_url: &str,
    cmd_name: &str,
    on_progress: impl Fn(&str),
) -> Result<PathBuf, AcpError> {
    if let Some(path) = find_cached_binary_for_agent(paths, agent_type, version, cmd_name)? {
        on_progress("Binary already cached, skipping download");
        return Ok(path);
    }

    let agent_id = agent_cache_key(agent_type);
    ensure_binary_with_progress(
        paths,
        &agent_id,
        version,
        archive_url,
        cmd_name,
        on_progress,
    )
    .await
}

async fn ensure_binary_with_progress(
    paths: &AgentStoragePaths,
    agent_id: &str,
    version: &str,
    archive_url: &str,
    cmd_name: &str,
    on_progress: impl Fn(&str),
) -> Result<PathBuf, AcpError> {
    if let Some(path) = find_cached_binary(paths, agent_id, version, cmd_name)? {
        return Ok(path);
    }

    let bin_name = if cfg!(target_os = "windows") {
        format!("{cmd_name}.exe")
    } else {
        cmd_name.to_string()
    };
    let operation_id = uuid::Uuid::new_v4();
    let staging_dir = paths
        .staging_dir()
        .join(format!("binary-{agent_id}-{operation_id}"));
    let archive_path = paths
        .downloads_dir()
        .join(format!("binary-{agent_id}-{operation_id}.archive"));
    std::fs::create_dir_all(paths.downloads_dir())
        .map_err(|e| AcpError::DownloadFailed(format!("failed to create downloads dir: {e}")))?;
    std::fs::create_dir_all(&staging_dir).map_err(|e| {
        AcpError::DownloadFailed(format!("failed to create binary staging dir: {e}"))
    })?;

    let result: Result<PathBuf, AcpError> = async {
        on_progress(&format!("Downloading {archive_url}"));
        download_file_with_progress(archive_url, &archive_path, &on_progress).await?;

        let extract_dir = staging_dir.join("extracted");
        std::fs::create_dir_all(&extract_dir)
            .map_err(|e| AcpError::DownloadFailed(format!("failed to create extract dir: {e}")))?;

        on_progress("Extracting archive...");
        if archive_url.ends_with(".tar.gz") || archive_url.ends_with(".tgz") {
            extract_tar_gz(&archive_path, &extract_dir)?;
        } else if archive_url.ends_with(".tar.bz2") || archive_url.ends_with(".tbz2") {
            extract_tar_bz2(&archive_path, &extract_dir)?;
        } else if archive_url.ends_with(".zip") {
            extract_zip(&archive_path, &extract_dir)?;
        } else {
            return Err(AcpError::DownloadFailed(format!(
                "unsupported archive format: {archive_url}"
            )));
        }

        on_progress("Locating binary...");
        let extracted_bin = find_binary_recursive(&extract_dir, &bin_name).ok_or_else(|| {
            AcpError::DownloadFailed(format!("binary '{bin_name}' not found in archive"))
        })?;

        let staged_path = staging_dir.join(&bin_name);
        std::fs::copy(&extracted_bin, &staged_path)
            .map_err(|e| AcpError::DownloadFailed(format!("failed to copy binary: {e}")))?;
        std::fs::remove_dir_all(&extract_dir).map_err(|e| {
            AcpError::DownloadFailed(format!("failed to finalize binary staging dir: {e}"))
        })?;
        let final_path = activate_staged_binary(paths, agent_id, version, &bin_name, &staging_dir)?;
        on_progress("Binary installed successfully");
        Ok(final_path)
    }
    .await;

    let _ = std::fs::remove_file(&archive_path);
    let _ = std::fs::remove_dir_all(&staging_dir);
    result
}

pub(crate) fn find_cached_binary(
    paths: &AgentStoragePaths,
    agent_id: &str,
    version: &str,
    cmd_name: &str,
) -> Result<Option<PathBuf>, AcpError> {
    Ok(installed_binary_path(paths, agent_id, version, cmd_name))
}

pub(crate) fn find_cached_binary_for_agent(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    cmd_name: &str,
) -> Result<Option<PathBuf>, AcpError> {
    let agent_id = agent_cache_key(agent_type);
    find_cached_binary(paths, &agent_id, version, cmd_name)
}

pub(crate) fn find_binary_recursive(dir: &PathBuf, name: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() && entry.file_name().to_string_lossy() == name {
            return Some(entry.into_path());
        }
    }
    None
}

async fn download_file_with_progress(
    url: &str,
    dest: &PathBuf,
    on_progress: &impl Fn(&str),
) -> Result<(), AcpError> {
    use futures_util::StreamExt;

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| AcpError::DownloadFailed(format!("HTTP request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AcpError::DownloadFailed(format!(
            "HTTP {} for {url}",
            response.status()
        )));
    }

    let total_size = response.content_length();
    let mut downloaded: u64 = 0;
    let mut last_reported_mb: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut file = std::fs::File::create(dest)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to create archive file: {e}")))?;

    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| AcpError::DownloadFailed(format!("failed to read chunk: {e}")))?;
        file.write_all(&chunk)
            .map_err(|e| AcpError::DownloadFailed(format!("failed to write archive: {e}")))?;
        downloaded += chunk.len() as u64;

        // Report progress every 1MB
        let current_mb = downloaded / (1024 * 1024);
        if current_mb > last_reported_mb {
            last_reported_mb = current_mb;
            if let Some(total) = total_size {
                let total_mb = total as f64 / (1024.0 * 1024.0);
                on_progress(&format!(
                    "Downloading... {current_mb:.0} MB / {total_mb:.1} MB"
                ));
            } else {
                on_progress(&format!("Downloading... {current_mb:.0} MB"));
            }
        }
    }

    if let Some(total) = total_size {
        let total_mb = total as f64 / (1024.0 * 1024.0);
        on_progress(&format!("Download complete ({total_mb:.1} MB)"));
    } else {
        let final_mb = downloaded as f64 / (1024.0 * 1024.0);
        on_progress(&format!("Download complete ({final_mb:.1} MB)"));
    }

    Ok(())
}

fn extract_tar_gz(archive: &PathBuf, dest: &PathBuf) -> Result<(), AcpError> {
    let file = std::fs::File::open(archive)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to open archive: {e}")))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to extract tar.gz: {e}")))?;
    Ok(())
}

fn extract_tar_bz2(archive: &PathBuf, dest: &PathBuf) -> Result<(), AcpError> {
    let file = std::fs::File::open(archive)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to open archive: {e}")))?;
    let bz = bzip2::read::BzDecoder::new(file);
    let mut tar = tar::Archive::new(bz);
    tar.unpack(dest)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to extract tar.bz2: {e}")))?;
    Ok(())
}

fn extract_zip(archive: &PathBuf, dest: &PathBuf) -> Result<(), AcpError> {
    let file = std::fs::File::open(archive)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to open archive: {e}")))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to read zip: {e}")))?;
    zip.extract(dest)
        .map_err(|e| AcpError::DownloadFailed(format!("failed to extract zip: {e}")))?;
    Ok(())
}

fn set_executable_permissions(path: &Path) -> Result<(), AcpError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| AcpError::DownloadFailed(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(|e| AcpError::DownloadFailed(e.to_string()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub(crate) fn is_binary_file_compatible(path: &Path) -> bool {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut header = [0_u8; 4];
    if file.read_exact(&mut header).is_err() {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        matches!(
            header,
            [0xFE, 0xED, 0xFA, 0xCE]
                | [0xCE, 0xFA, 0xED, 0xFE]
                | [0xFE, 0xED, 0xFA, 0xCF]
                | [0xCF, 0xFA, 0xED, 0xFE]
                | [0xCA, 0xFE, 0xBA, 0xBE]
                | [0xBE, 0xBA, 0xFE, 0xCA]
                | [0xCA, 0xFE, 0xBA, 0xBF]
                | [0xBF, 0xBA, 0xFE, 0xCA]
        )
    }

    #[cfg(target_os = "linux")]
    {
        header == [0x7F, b'E', b'L', b'F']
    }

    #[cfg(target_os = "windows")]
    {
        header[0] == b'M' && header[1] == b'Z'
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_uv_paths_are_resolved_next_to_application_binary() {
        let executable = PathBuf::from("C:/Program Files/iyw-claw/iyw-claw.exe");
        let (uv, uvx) = bundled_uv_tool_paths(&executable).expect("application directory");

        assert_eq!(uv, PathBuf::from("C:/Program Files/iyw-claw/uv.exe"));
        assert_eq!(uvx, PathBuf::from("C:/Program Files/iyw-claw/uvx.exe"));
    }
    use crate::acp::agent_storage::AgentStoragePaths;

    fn write_compatible_test_binary(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        #[cfg(target_os = "windows")]
        let bytes = [b'M', b'Z', 0, 0];
        #[cfg(target_os = "linux")]
        let bytes = [0x7f, b'E', b'L', b'F'];
        #[cfg(target_os = "macos")]
        let bytes = [0xfe, 0xed, 0xfa, 0xcf];
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        let bytes = [0, 0, 0, 0];
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn cache_key_uses_registry_id() {
        assert_eq!(agent_cache_key(AgentType::OpenCode), "opencode");
        assert_eq!(agent_cache_key(AgentType::Codex), "codex-acp");
    }

    #[test]
    fn version_normalization_is_consistent() {
        assert_eq!(normalize_version_label("v1.2.15"), "1.2.15");
        assert_eq!(normalize_version_label("V0.9.4 "), "0.9.4");
        assert_eq!(normalize_version_label("1.25.1"), "1.25.1");
    }

    #[test]
    fn private_binary_and_uv_paths_stay_below_agent_storage() {
        let root = PathBuf::from("D:/iyw-claw-data");
        let paths = AgentStoragePaths::new(root.clone());
        let platform = registry::current_platform();

        assert_eq!(
            binary_dir_for(&paths, "opencode", "v1.2.15").unwrap(),
            root.join("runtime")
                .join("binary")
                .join("opencode")
                .join("1.2.15")
                .join(platform)
        );
        assert_eq!(
            uv_tool_dir_for(&paths),
            root.join("runtime")
                .join("uv")
                .join(UV_TOOL_VERSION)
                .join(platform)
        );
        assert_eq!(
            managed_uv_tool_path(&paths, "uvx"),
            uv_tool_dir_for(&paths).join(if cfg!(windows) { "uvx.exe" } else { "uvx" })
        );
        assert!(uvx_prepared_marker_for(&paths, "hermes").starts_with(&root));
        assert!(binary_trash_dir(&paths).starts_with(&root));

        if let Some(system_cache) = dirs::cache_dir() {
            assert!(!binary_dir_for(&paths, "opencode", "1.2.15")
                .unwrap()
                .starts_with(&system_cache));
            assert!(!uv_tool_dir_for(&paths).starts_with(system_cache));
        }
    }

    #[test]
    fn uv_runtime_environment_is_private() {
        let root = PathBuf::from("D:/iyw-claw-data");
        let paths = AgentStoragePaths::new(root.clone());
        let env = uv_runtime_env(&paths);

        assert_eq!(env.get("UV_CACHE_DIR"), Some(&paths.uv_cache_dir()));
        assert_eq!(
            env.get("UV_TOOL_DIR"),
            Some(&root.join("runtime").join("uv").join("tools"))
        );
        assert_eq!(
            env.get("UV_TOOL_BIN_DIR"),
            Some(&root.join("runtime").join("uv").join("bin"))
        );
    }

    #[test]
    fn binary_lookup_reads_only_the_supplied_private_root() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AgentStoragePaths::new(temp.path().join("private"));
        let binary = binary_dir_for(&paths, "opencode", "1.2.15")
            .unwrap()
            .join(if cfg!(windows) {
                "opencode.exe"
            } else {
                "opencode"
            });
        write_compatible_test_binary(&binary);

        let found = find_best_cached_binary_for_agent(&paths, AgentType::OpenCode, "opencode")
            .unwrap()
            .unwrap();
        assert_eq!(found, (binary, "1.2.15".to_string()));
    }

    #[test]
    fn uvx_marker_round_trip_uses_private_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AgentStoragePaths::new(temp.path().join("private"));

        mark_uvx_agent_prepared(&paths, AgentType::Hermes, "0.16.0").unwrap();
        assert_eq!(
            uvx_prepared_version(&paths, AgentType::Hermes).as_deref(),
            Some("0.16.0")
        );
        assert!(uvx_prepared_marker_for(&paths, "hermes").starts_with(paths.root()));

        clear_uvx_agent_prepared(&paths, AgentType::Hermes).unwrap();
        assert_eq!(uvx_prepared_version(&paths, AgentType::Hermes), None);
    }

    #[test]
    fn invalid_staged_upgrade_keeps_previous_binary_and_cleans_staging() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AgentStoragePaths::new(temp.path().join("private"));
        let executable = if cfg!(windows) {
            "opencode.exe"
        } else {
            "opencode"
        };
        let previous = binary_dir_for(&paths, "opencode", "1.2.15")
            .unwrap()
            .join(executable);
        write_compatible_test_binary(&previous);

        let staging = paths.staging_dir().join("invalid-upgrade");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join(executable), b"invalid").unwrap();

        assert!(activate_staged_binary(&paths, "opencode", "1.3.0", executable, &staging).is_err());
        assert!(previous.is_file());
        assert!(!binary_dir_for(&paths, "opencode", "1.3.0")
            .unwrap()
            .exists());
        assert!(!staging.exists());
    }
}
