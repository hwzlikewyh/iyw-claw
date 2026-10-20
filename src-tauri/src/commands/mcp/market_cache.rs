use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;

const MARKER: &str = ".iyw-artifact";
const BUFFER_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize, PartialEq)]
struct Inventory {
    digest: String,
    files: BTreeMap<String, String>,
}

pub(super) fn valid(path: &Path, digest: &str) -> bool {
    let Ok(raw) = std::fs::read(path.join(MARKER)) else {
        return false;
    };
    let Ok(saved) = serde_json::from_slice::<Inventory>(&raw) else {
        return false;
    };
    inventory(path, digest).is_ok_and(|actual| saved == actual)
}

pub(super) fn write(path: &Path, digest: &str) -> Result<(), AppCommandError> {
    let value = inventory(path, digest)?;
    let bytes = serde_json::to_vec(&value).map_err(|_| invalid())?;
    std::fs::write(path.join(MARKER), bytes).map_err(|_| invalid())
}

fn inventory(root: &Path, digest: &str) -> Result<Inventory, AppCommandError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = BTreeMap::new();
    let mut expanded = 0_u64;
    let mut entries = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).map_err(|_| invalid())? {
            let entry = entry.map_err(|_| invalid())?;
            let path = entry.path();
            if path == root.join(MARKER) {
                continue;
            }
            entries += 1;
            if entries > super::market_artifact::MAX_FILES {
                return Err(invalid());
            }
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| invalid())?;
            if metadata.file_type().is_symlink() {
                return Err(invalid());
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            expanded = expanded.saturating_add(metadata.len());
            if !metadata.is_file() || expanded > super::market_artifact::MAX_EXPANDED {
                return Err(invalid());
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| invalid())?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, hash(&path)?);
        }
    }
    Ok(Inventory {
        digest: digest.to_owned(),
        files,
    })
}

fn hash(path: &Path) -> Result<String, AppCommandError> {
    let mut file = std::fs::File::open(path).map_err(|_| invalid())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer).map_err(|_| invalid())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > super::market_artifact::MAX_EXPANDED {
            return Err(invalid());
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn invalid() -> AppCommandError {
    AppCommandError::configuration_invalid("连接器受管文件校验失败，请检查安装目录")
}
