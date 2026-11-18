//! 技能内容哈希的进程内缓存：文件元数据未变时不重读文件内容。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::{normalize_path, TreeEntry};

/// 缓存条目上限；超过后整体清空，避免长时间运行时无限增长。
const MAX_ENTRIES: usize = 512;
const MAX_AGE: Duration = Duration::from_secs(60);

/// 技能路径 -> (元数据指纹, 内容哈希)。
type Entries = HashMap<PathBuf, (String, String, Instant)>;

fn entries() -> MutexGuard<'static, Entries> {
    static CACHE: OnceLock<Mutex<Entries>> = OnceLock::new();
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// 由各文件的相对路径、长度和修改时间生成指纹；任一文件读不到修改时间时不缓存。
pub(super) fn fingerprint(files: &[TreeEntry]) -> Option<String> {
    let mut hasher = Sha256::new();
    for file in files {
        let modified = file.modified?.duration_since(UNIX_EPOCH).ok()?;
        hasher.update(normalize_path(&file.relative).as_bytes());
        hasher.update(b"\0");
        hasher.update(file.length.to_le_bytes());
        hasher.update(modified.as_nanos().to_le_bytes());
    }
    Some(format!("{:x}", hasher.finalize()))
}

pub(super) fn lookup(path: &Path, fingerprint: &str) -> Option<String> {
    entries()
        .get(path)
        .filter(|(known, _, stored)| known == fingerprint && stored.elapsed() < MAX_AGE)
        .map(|(_, hash, _)| hash.clone())
}

pub(super) fn store(path: &Path, fingerprint: String, hash: &str) {
    let mut cache = entries();
    if cache.len() >= MAX_ENTRIES && !cache.contains_key(path) {
        cache.clear();
    }
    cache.insert(
        path.to_path_buf(),
        (fingerprint, hash.to_string(), Instant::now()),
    );
}
