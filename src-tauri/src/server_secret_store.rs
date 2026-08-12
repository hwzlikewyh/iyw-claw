#![cfg(not(feature = "tauri-runtime"))]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const LOCK_WAIT: Duration = Duration::from_secs(10);
const LOCK_RETRY: Duration = Duration::from_millis(25);

fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn get(key: &str) -> Option<String> {
    let _guard = store_lock().lock().ok()?;
    read_tokens().get(key).cloned()
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    update(|tokens| {
        tokens.insert(key.to_string(), value.to_string());
    })
}

pub fn delete(key: &str) -> Result<(), String> {
    update(|tokens| {
        tokens.remove(key);
    })
}

pub fn get_or_insert(key: &str, value: &str) -> Result<String, String> {
    let _guard = store_lock()
        .lock()
        .map_err(|_| "token store lock unavailable".to_string())?;
    let _file_guard = TokenFileLock::acquire()?;
    let mut tokens = read_tokens();
    if let Some(existing) = tokens.get(key) {
        return Ok(existing.clone());
    }
    tokens.insert(key.to_string(), value.to_string());
    write_tokens(&tokens)?;
    Ok(value.to_string())
}

fn update(mutate: impl FnOnce(&mut HashMap<String, String>)) -> Result<(), String> {
    let _guard = store_lock()
        .lock()
        .map_err(|_| "token store lock unavailable".to_string())?;
    let _file_guard = TokenFileLock::acquire()?;
    let mut tokens = read_tokens();
    mutate(&mut tokens);
    write_tokens(&tokens)
}

struct TokenFileLock {
    path: PathBuf,
}

impl TokenFileLock {
    fn acquire() -> Result<Self, String> {
        let path = tokens_file_path().with_extension("lock");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(file_error)?;
        }
        let started = Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    write!(file, "{}", std::process::id()).map_err(file_error)?;
                    file.sync_all().map_err(file_error)?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if stale_lock(&path) {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if started.elapsed() >= LOCK_WAIT {
                        return Err("token store lock timeout".to_string());
                    }
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(error) => return Err(file_error(error)),
            }
        }
    }
}

impl Drop for TokenFileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn stale_lock(path: &Path) -> bool {
    let mut pid = String::new();
    if std::fs::File::open(path)
        .and_then(|mut file| file.read_to_string(&mut pid))
        .is_err()
    {
        return false;
    }
    let Ok(pid) = pid.trim().parse::<u32>() else {
        return false;
    };
    let system = sysinfo::System::new_all();
    system.process(sysinfo::Pid::from_u32(pid)).is_none()
}

fn tokens_file_path() -> PathBuf {
    let dir = std::env::var("IYW_CLAW_DATA_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|path| path.join("iyw-claw")))
        .unwrap_or_else(|| PathBuf::from(".iyw-claw-data"));
    crate::git_credential::absolutize(&dir).join("tokens.json")
}

fn read_tokens() -> HashMap<String, String> {
    std::fs::read_to_string(tokens_file_path())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn write_tokens(tokens: &HashMap<String, String>) -> Result<(), String> {
    let path = tokens_file_path();
    let parent = path
        .parent()
        .ok_or_else(|| "token store has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(file_error)?;
    let json = serde_json::to_vec_pretty(tokens).map_err(file_error)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    temp.write_all(&json).map_err(file_error)?;
    temp.as_file().sync_all().map_err(file_error)?;
    let temp_path = temp.into_temp_path();
    replace_file(&temp_path, &path)
}

#[cfg(unix)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(temp, std::fs::Permissions::from_mode(0o600)).map_err(file_error)?;
    std::fs::rename(temp, target).map_err(file_error)
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
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
    let result = unsafe {
        MoveFileExW(
            wide(temp).as_ptr(),
            wide(target).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(file_error(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(temp, target).map_err(file_error)
}

fn file_error(error: impl std::fmt::Display) -> String {
    format!("token store operation failed: {error}")
}
