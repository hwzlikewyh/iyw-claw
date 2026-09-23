use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use sha2::{Digest, Sha256};

use super::file_stamp::FileStamp;
use super::ManagedFile;

const MAX_CACHED_FILES: usize = 64;
const HASH_BUFFER_BYTES: usize = 64 * 1024;
type Verification = Arc<Mutex<Option<(FileStamp, String)>>>;
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Verification>>> = OnceLock::new();

pub(super) fn clear() {
    if let Some(cache) = CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}

fn entry(path: &Path) -> Verification {
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(entry) = cache.get(path) {
        return Arc::clone(entry);
    }
    if cache.len() >= MAX_CACHED_FILES {
        cache.clear();
    }
    Arc::clone(cache.entry(path.to_path_buf()).or_default())
}

pub(super) fn verify(file: &mut File, path: &Path, expected: &ManagedFile) -> bool {
    let entry = entry(path);
    let mut cached = entry.lock().unwrap_or_else(|error| error.into_inner());
    let before = FileStamp::read(file);
    if cached
        .as_ref()
        .is_some_and(|(stamp, digest)| before.as_ref() == Some(stamp) && digest == &expected.sha256)
    {
        return true;
    }
    *cached = None;
    if !hash_file(file).is_some_and(|digest| digest == expected.sha256) {
        return false;
    }
    let after = FileStamp::read(file);
    if before != after {
        return false;
    }
    // 无可靠文件签名时仍完整校验，但不复用结果。
    if let Some(stamp) = after {
        *cached = Some((stamp, expected.sha256.clone()));
    }
    true
}

fn hash_file(file: &mut File) -> Option<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}
