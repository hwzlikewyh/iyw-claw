use std::fs;
use std::path::{Path, PathBuf};

use super::conversation_history_cache::HistoryCacheIndex;

const MAX_CACHE_CONVERSATIONS: usize = 64;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn remove_old_generations(root: &Path, conversation_id: i32, keep: &str) {
    let prefix = format!("{conversation_id}-");
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name != keep {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub(super) fn prune(root: &Path) {
    let mut entries = cache_entries(root);
    entries.sort_by_key(|entry| entry.0);
    let mut total_bytes = entries.iter().map(|entry| entry.1).sum::<u64>();
    while entries.len() > MAX_CACHE_CONVERSATIONS || total_bytes > MAX_CACHE_BYTES {
        let (_, size, index_path, directory) = entries.remove(0);
        total_bytes = total_bytes.saturating_sub(size);
        let _ = fs::remove_file(index_path);
        let _ = fs::remove_dir_all(root.join(directory));
    }
}

fn cache_entries(root: &Path) -> Vec<(std::time::SystemTime, u64, PathBuf, String)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".index.json"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            let index: HistoryCacheIndex =
                serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
            let size = directory_size(&root.join(&index.directory));
            Some((modified, size, entry.path(), index.directory))
        })
        .collect()
}

fn directory_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}
