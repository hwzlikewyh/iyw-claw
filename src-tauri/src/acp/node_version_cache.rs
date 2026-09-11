use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;

const MAX_ENTRIES: usize = 8;
const CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Eq, Hash, PartialEq)]
struct ProbeKey {
    executable: std::path::PathBuf,
    length: u64,
    modified: SystemTime,
    environment: [u8; 32],
}

struct ProbeEntry {
    created: Instant,
    version: OnceCell<String>,
}

fn cache() -> &'static Mutex<HashMap<ProbeKey, Arc<ProbeEntry>>> {
    static CACHE: OnceLock<Mutex<HashMap<ProbeKey, Arc<ProbeEntry>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub(super) fn clear() {
    cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

fn env_key(key: OsString) -> OsString {
    #[cfg(windows)]
    return key.to_string_lossy().to_uppercase().into();
    #[cfg(not(windows))]
    key
}

fn probe_key(path: &Path, command: &tokio::process::Command) -> Option<ProbeKey> {
    let executable = path.canonicalize().ok()?;
    let metadata = std::fs::metadata(&executable).ok()?;
    let mut environment: BTreeMap<_, _> = std::env::vars_os()
        .map(|(key, value)| (env_key(key), value))
        .collect();
    for (key, value) in command.as_std().get_envs() {
        let key = env_key(key.to_owned());
        if let Some(value) = value {
            environment.insert(key, value.to_owned());
        } else {
            environment.remove(&key);
        }
    }
    // 只保留环境摘要，不将凭据或完整环境放进全局缓存/日志。
    let mut digest = Sha256::new();
    for (key, value) in environment {
        for part in [key, value] {
            digest.update(part.as_encoded_bytes().len().to_le_bytes());
            digest.update(part.as_encoded_bytes());
        }
    }
    Some(ProbeKey {
        executable,
        length: metadata.len(),
        modified: metadata.modified().ok()?,
        environment: digest.finalize().into(),
    })
}

fn entry_for(key: ProbeKey) -> Arc<ProbeEntry> {
    let mut entries = cache().lock().unwrap_or_else(|error| error.into_inner());
    entries.retain(|_, entry| entry.created.elapsed() < CACHE_TTL);
    if let Some(entry) = entries.get(&key) {
        return Arc::clone(entry);
    }
    if entries.len() >= MAX_ENTRIES {
        if let Some(oldest) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.created)
            .map(|(key, _)| key.clone())
        {
            entries.remove(&oldest);
        }
    }
    let entry = Arc::new(ProbeEntry {
        created: Instant::now(),
        version: OnceCell::new(),
    });
    entries.insert(key, Arc::clone(&entry));
    entry
}

pub(super) async fn version(
    path: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut command = crate::process::tokio_command(path);
    command
        .envs(environment)
        .arg("--version")
        .kill_on_drop(true);
    let Some(key) = probe_key(path, &command) else {
        return probe(&mut command).await;
    };
    let entry = entry_for(key.clone());
    // 同一可执行文件和环境的并发启动只探测一次；失败/取消不缓存。
    let result = entry
        .version
        .get_or_try_init(|| probe(&mut command))
        .await
        .cloned();
    if result.is_err() || probe_key(path, &command).as_ref() != Some(&key) {
        let mut entries = cache().lock().unwrap_or_else(|error| error.into_inner());
        if entries
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, &entry))
        {
            entries.remove(&key);
        }
    }
    result
}

async fn probe(command: &mut tokio::process::Command) -> Result<String, String> {
    let output = command
        .output()
        .await
        .map_err(|error| format!("failed to execute Node.js for version check: {error}"))?;
    if !output.status.success() {
        return Err("Node.js version check failed".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
