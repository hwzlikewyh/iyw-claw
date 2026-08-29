use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::error::{BrowserError, BrowserErrorCode};
use super::process::{
    capture_process, find_processes_by_executable_arg, kill_tree_checked, process_matches,
    process_matches_executable, wait_for_exit, ProcessRecord,
};

#[derive(Debug)]
pub(super) struct ProfileGuard {
    lock_path: PathBuf,
    runtime_id: String,
    pub profile_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockRecord {
    runtime_id: String,
    pid: u32,
    process_started_at: u64,
    #[serde(default)]
    daemon: Option<LockedProcess>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedProcess {
    pid: u32,
    process_started_at: u64,
}

impl ProfileGuard {
    pub async fn reclaim_stale(
        root: &Path,
        sidecar_path: &Path,
        engine_path: &Path,
    ) -> Result<usize, BrowserError> {
        let lock_path = root.join("runtime.lock.json");
        match std::fs::metadata(&lock_path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(_) => return Err(profile_error()),
        }
        let profile_path = root.join("profile-v1");
        reclaim_stale_lock(&lock_path, sidecar_path, engine_path, &profile_path).await
    }

    pub async fn acquire(
        root: &Path,
        runtime_id: &str,
        sidecar_path: &Path,
        engine_path: &Path,
    ) -> Result<Self, BrowserError> {
        std::fs::create_dir_all(root).map_err(|_| profile_error())?;
        let profile_path = root.join("profile-v1");
        std::fs::create_dir_all(&profile_path).map_err(|_| profile_error())?;
        let lock_path = root.join("runtime.lock.json");
        let current = capture_process(std::process::id(), "iyw-claw").ok_or_else(profile_error)?;
        let record = LockRecord {
            runtime_id: runtime_id.to_string(),
            pid: current.pid,
            process_started_at: current.started_at,
            daemon: None,
        };
        match write_new_lock(&lock_path, &record) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                reclaim_stale_lock(&lock_path, sidecar_path, engine_path, &profile_path).await?;
                write_new_lock(&lock_path, &record).map_err(|_| profile_locked())?;
            }
            Err(_) => return Err(profile_error()),
        }
        Ok(Self {
            lock_path,
            runtime_id: runtime_id.to_string(),
            profile_path,
        })
    }

    pub fn bind_daemon(&self, daemon: &ProcessRecord) -> Result<(), BrowserError> {
        let mut record = read_lock(&self.lock_path)?;
        if record.runtime_id != self.runtime_id {
            return Err(profile_locked());
        }
        record.daemon = Some(LockedProcess {
            pid: daemon.pid,
            process_started_at: daemon.started_at,
        });
        if replace_lock(&self.lock_path, &record).is_err() {
            return Err(profile_error());
        }
        Ok(())
    }

    pub async fn seed_user_profile(
        &self,
        source: &Path,
        browser_executable: &Path,
    ) -> Result<(), BrowserError> {
        super::profile_seed::seed(&self.profile_path, source, browser_executable).await
    }
}

impl Drop for ProfileGuard {
    fn drop(&mut self) {
        let owned = std::fs::read(&self.lock_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<LockRecord>(&bytes).ok())
            .is_some_and(|record| record.runtime_id == self.runtime_id);
        if owned {
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }
}

fn write_new_lock(path: &Path, record: &LockRecord) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing lock parent"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)
        .map(|_| ())
        .map_err(|error| error.error)
}

async fn reclaim_stale_lock(
    path: &Path,
    sidecar_path: &Path,
    engine_path: &Path,
    profile_path: &Path,
) -> Result<usize, BrowserError> {
    let record = read_lock(path)?;
    let process = ProcessRecord {
        pid: record.pid,
        started_at: record.process_started_at,
        label: "profile-owner".to_string(),
        executable: None,
    };
    if process_matches(&process) {
        return Err(profile_locked());
    }
    if record.runtime_id.len() != 32 || !record.runtime_id.chars().all(|ch| ch.is_ascii_hexdigit())
    {
        return Err(profile_locked());
    }
    let root = path.parent().ok_or_else(profile_locked)?;
    let runtime_dir = root.join(format!("runtime-{}", record.runtime_id));
    let session = format!("iyw-runtime-{}", &record.runtime_id[..12]);
    let mut stale = Vec::new();
    if let Some(daemon) = record.daemon {
        stale.push(locked_process(daemon, sidecar_path));
    } else if let Some(daemon) =
        discover_published_daemon(&runtime_dir, &session, sidecar_path).await
    {
        stale.push(daemon);
    }
    stale.extend(find_processes_by_executable_arg(
        sidecar_path,
        &session,
        "stale-browser-controller",
    ));
    stale.extend(find_processes_by_executable_arg(
        engine_path,
        &profile_path.to_string_lossy(),
        "stale-browser-engine",
    ));
    let reclaimed = cleanup_stale_processes(stale).await?;
    let _ = tokio::fs::remove_dir_all(runtime_dir).await;
    std::fs::remove_file(path).map_err(|_| profile_locked())?;
    Ok(reclaimed)
}

fn locked_process(process: LockedProcess, sidecar_path: &Path) -> ProcessRecord {
    ProcessRecord {
        pid: process.pid,
        started_at: process.process_started_at,
        label: "stale-browser-runtime".to_string(),
        executable: Some(sidecar_path.to_path_buf()),
    }
}

async fn discover_published_daemon(
    runtime_dir: &Path,
    session: &str,
    sidecar_path: &Path,
) -> Option<ProcessRecord> {
    let path = runtime_dir.join("sockets").join(format!("{session}.pid"));
    for _ in 0..5 {
        if let Ok(value) = tokio::fs::read_to_string(&path).await {
            if let Ok(pid) = value.trim().parse::<u32>() {
                if let Some(mut process) = capture_process(pid, "stale-browser-runtime") {
                    if process_matches_executable(&process, sidecar_path) {
                        process.executable = Some(sidecar_path.to_path_buf());
                        return Some(process);
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

async fn cleanup_stale_processes(mut processes: Vec<ProcessRecord>) -> Result<usize, BrowserError> {
    processes.sort_by_key(|process| (process.pid, process.started_at));
    processes.dedup_by_key(|process| (process.pid, process.started_at));
    let mut reclaimed = 0;
    for process in processes {
        if !process_matches(&process) {
            continue;
        }
        kill_tree_checked(&process)
            .await
            .map_err(|_| profile_locked())?;
        if !wait_for_exit(&process, Duration::from_secs(2)).await {
            return Err(profile_locked());
        }
        reclaimed += 1;
    }
    Ok(reclaimed)
}

fn read_lock(path: &Path) -> Result<LockRecord, BrowserError> {
    let bytes = std::fs::read(path).map_err(|_| profile_locked())?;
    serde_json::from_slice(&bytes).map_err(|_| profile_locked())
}

fn replace_lock(path: &Path, record: &LockRecord) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing lock parent"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    let temp = temp.into_temp_path();
    replace_file(&temp, path)
}

#[cfg(unix)]
fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::rename(source, target)
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let source = wide(source);
    let target = wide(target);
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    (result != 0)
        .then_some(())
        .ok_or_else(std::io::Error::last_os_error)
}

#[cfg(not(any(unix, windows)))]
fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::rename(source, target)
}

fn profile_locked() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserProfileLocked,
        "The iyw-claw browser profile is already in use",
    )
    .retryable(true)
}

fn profile_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser profile could not be prepared",
    )
    .retryable(true)
}
