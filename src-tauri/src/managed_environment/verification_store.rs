use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::file_stamp::FileStamp;

const STORE_FILE: &str = "inventory/entrypoint-verification.json";
const MAX_STORED_FILES: usize = 64;

#[derive(Clone, Serialize, Deserialize)]
struct Verified {
    stamp: FileStamp,
    sha256: String,
}

type Store = HashMap<String, Verified>;
static STORE: OnceLock<Mutex<Option<Store>>> = OnceLock::new();

fn store_path() -> PathBuf {
    super::location::root().join(STORE_FILE)
}

fn with_store<T>(action: impl FnOnce(&mut Store) -> T) -> T {
    let mut guard = STORE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let store = guard.get_or_insert_with(|| {
        std::fs::read(store_path())
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default()
    });
    action(store)
}

/// 之前的进程已对同一文件签名完成校验时返回 true，冷启动可跳过整文件哈希。
pub(super) fn is_verified(path: &Path, stamp: &FileStamp, sha256: &str) -> bool {
    let key = path.to_string_lossy();
    with_store(|store| {
        store
            .get(key.as_ref())
            .is_some_and(|entry| &entry.stamp == stamp && entry.sha256 == sha256)
    })
}

pub(super) fn record(path: &Path, stamp: &FileStamp, sha256: &str) {
    with_store(|store| {
        if store.len() >= MAX_STORED_FILES {
            store.clear();
        }
        let entry = Verified {
            stamp: stamp.clone(),
            sha256: sha256.to_string(),
        };
        store.insert(path.to_string_lossy().into_owned(), entry);
        if let Err(error) = write(store) {
            tracing::warn!(%error, "[environment] entrypoint verification record write failed");
        }
    });
}

pub(super) fn clear() {
    with_store(|store| {
        store.clear();
        let _ = std::fs::remove_file(store_path());
    });
}

fn write(store: &Store) -> std::io::Result<()> {
    let path = store_path();
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec(store)?)?;
    std::fs::rename(&temporary, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}
