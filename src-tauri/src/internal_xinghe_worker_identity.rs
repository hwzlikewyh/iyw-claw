use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

const IDENTITY: &[u8] =
    b"IYW_XINGHE_WORKER|1|0.153.4|3d2ee51ca2d5db578f328aa75e20aa22c0197c9a|END_WORKER_ID\0";
const READ_BYTES: usize = 64 * 1024;
type CachedIdentity = (u64, Option<SystemTime>, bool);
static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedIdentity>>> = OnceLock::new();

pub(super) fn validate(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|_| "无法读取内置星河运行时".to_string())?;
    let signature = (metadata.len(), metadata.modified().ok());
    let cache = CACHE.get_or_init(Default::default);
    if let Some((length, modified, valid)) = cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(path)
    {
        if (*length, *modified) == signature {
            return outcome(*valid);
        }
    }
    let valid = contains_identity(path).map_err(|_| "无法校验内置星河运行时".to_string())?;
    cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(path.to_path_buf(), (signature.0, signature.1, valid));
    outcome(valid)
}

fn contains_identity(path: &Path) -> std::io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; READ_BYTES + IDENTITY.len()];
    let mut kept = 0;
    loop {
        let count = file.read(&mut buffer[kept..])?;
        if count == 0 {
            return Ok(false);
        }
        let end = kept + count;
        if buffer[..end]
            .windows(IDENTITY.len())
            .any(|window| window == IDENTITY)
        {
            return Ok(true);
        }
        kept = end.min(IDENTITY.len() - 1);
        buffer.copy_within(end - kept..end, 0);
    }
}

fn outcome(valid: bool) -> Result<(), String> {
    if valid {
        Ok(())
    } else {
        Err("内置星河运行时与应用版本不匹配，请修复安装".into())
    }
}
