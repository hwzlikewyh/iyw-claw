#![cfg(not(feature = "tauri-runtime"))]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const LOCK_WAIT: Duration = Duration::from_secs(10);
const LOCK_RETRY: Duration = Duration::from_millis(25);
const STORE_VERSION: u8 = 1;

#[derive(Deserialize, Serialize)]
struct EncryptedStore {
    version: u8,
    entries: HashMap<String, String>,
}

fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn get(key: &str) -> Option<String> {
    let _guard = store_lock().lock().ok()?;
    let _file_guard = TokenFileLock::acquire().ok()?;
    match read_and_migrate() {
        Ok(tokens) => tokens.get(key).cloned(),
        Err(error) => {
            tracing::error!(error = %error, "secret store read failed");
            None
        }
    }
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
        .map_err(|_| "secret store lock unavailable".to_string())?;
    let _file_guard = TokenFileLock::acquire()?;
    let mut tokens = read_and_migrate()?;
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
        .map_err(|_| "secret store lock unavailable".to_string())?;
    let _file_guard = TokenFileLock::acquire()?;
    let mut tokens = read_and_migrate()?;
    mutate(&mut tokens);
    write_tokens(&tokens)
}

fn read_and_migrate() -> Result<HashMap<String, String>, String> {
    let (tokens, needs_migration) = read_tokens()?;
    if needs_migration {
        write_tokens(&tokens)?;
        tracing::info!("migrated legacy plaintext secret store");
    }
    Ok(tokens)
}

fn read_tokens() -> Result<(HashMap<String, String>, bool), String> {
    let path = tokens_file_path();
    let json = match std::fs::read_to_string(path) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((HashMap::new(), false));
        }
        Err(error) => return Err(file_error(error)),
    };
    let value: serde_json::Value = serde_json::from_str(&json).map_err(file_error)?;
    if value.get("version").is_some() || value.get("entries").is_some() {
        let store: EncryptedStore = serde_json::from_value(value).map_err(file_error)?;
        if store.version != STORE_VERSION {
            return Err("secret store version is unsupported".to_string());
        }
        return decrypt_tokens(store.entries).map(|tokens| (tokens, false));
    }
    let tokens = serde_json::from_value(value).map_err(file_error)?;
    Ok((tokens, true))
}

fn decrypt_tokens(stored: HashMap<String, String>) -> Result<HashMap<String, String>, String> {
    let mut tokens = HashMap::with_capacity(stored.len());
    for (key, value) in stored {
        let plaintext = crate::server_channel_target_crypto::decrypt_store_value(&key, &value)?;
        tokens.insert(key, plaintext);
    }
    Ok(tokens)
}

fn write_tokens(tokens: &HashMap<String, String>) -> Result<(), String> {
    let path = tokens_file_path();
    let parent = path
        .parent()
        .ok_or_else(|| "secret store has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(file_error)?;
    let encrypted = EncryptedStore {
        version: STORE_VERSION,
        entries: encrypt_tokens(tokens)?,
    };
    let json = serde_json::to_vec_pretty(&encrypted).map_err(file_error)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    temp.write_all(&json).map_err(file_error)?;
    temp.as_file().sync_all().map_err(file_error)?;
    let temp_path = temp.into_temp_path();
    replace_file(&temp_path, &path)
}

fn encrypt_tokens(tokens: &HashMap<String, String>) -> Result<HashMap<String, String>, String> {
    tokens
        .iter()
        .map(|(key, value)| {
            crate::server_channel_target_crypto::encrypt_store_value(key, value)
                .map(|encrypted| (key.clone(), encrypted))
        })
        .collect()
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
            match create_lock(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if stale_lock(&path) {
                        let _ = std::fs::remove_file(&path);
                    } else if started.elapsed() >= LOCK_WAIT {
                        return Err("secret store lock timeout".to_string());
                    } else {
                        std::thread::sleep(LOCK_RETRY);
                    }
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

fn create_lock(path: &Path) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    write!(file, "{}", std::process::id())?;
    file.sync_all()
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

pub(crate) fn store_dir() -> PathBuf {
    let dir = std::env::var("IYW_CLAW_DATA_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|path| path.join("iyw-claw")))
        .unwrap_or_else(|| PathBuf::from(".iyw-claw-data"));
    crate::git_credential::absolutize(&dir)
}

fn tokens_file_path() -> PathBuf {
    store_dir().join("tokens.json")
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
    format!("secret store operation failed: {error}")
}
