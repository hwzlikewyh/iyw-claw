use super::billing::{self, Receipts};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use walkdir::WalkDir;

const PATH_CACHE_LIMIT: usize = 512;
type Paths = HashMap<(PathBuf, String), PathBuf>;
type Located = (HashMap<String, PathBuf>, Vec<PathBuf>);

fn path_cache() -> &'static Mutex<Paths> {
    static PATHS: OnceLock<Mutex<Paths>> = OnceLock::new();
    PATHS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl super::CodexParser {
    /// 已验证路径可复用；有新子代理时只增加一次目录扫描，不缓存不存在的文件。
    pub(crate) fn billing_receipts_batch(
        &self,
        ids: &HashSet<String>,
    ) -> HashMap<String, Receipts> {
        let (paths, billing_paths) = locate(&self.base_dir, ids);
        let mut result = HashMap::new();
        for (id, path) in paths {
            if let Some(mut receipts) = billing::read(&path, &id) {
                remember(&self.base_dir, &id, &path);
                if receipts.requests.is_empty() {
                    for source in &billing_paths {
                        if source
                            .file_name()
                            .is_some_and(|name| name.to_string_lossy().contains(&id))
                        {
                            billing::read_requests(source, &id, &mut receipts);
                        }
                    }
                }
                result.insert(id, receipts);
            }
        }
        result
    }
}

fn locate(base: &Path, ids: &HashSet<String>) -> Located {
    let mut paths = HashMap::new();
    if let Ok(cached) = path_cache().lock() {
        for id in ids {
            if let Some(path) = cached
                .get(&(base.to_path_buf(), id.clone()))
                .filter(|path| path.is_file())
            {
                paths.insert(id.clone(), path.clone());
            }
        }
    }
    let missing: HashSet<_> = ids
        .iter()
        .filter(|id| !paths.contains_key(*id))
        .cloned()
        .collect();
    let mut sidecars = Vec::new();
    if !missing.is_empty() {
        let mut roots = vec![base.to_path_buf()];
        if let Some(parent) = base.parent() {
            roots.push(parent.join("archived_sessions"));
        }
        scan(roots, &missing, (&mut paths, &mut sidecars));
    }
    (paths, sidecars)
}

fn scan(
    roots: Vec<PathBuf>,
    ids: &HashSet<String>,
    output: (&mut HashMap<String, PathBuf>, &mut Vec<PathBuf>),
) {
    for root in roots {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if !name.ends_with(".jsonl") {
                continue;
            }
            if name.ends_with(".billing.jsonl") {
                output.1.push(entry.path().to_path_buf());
                continue;
            }
            if let Some(id) = ids.iter().find(|id| name.contains(id.as_str())) {
                output
                    .0
                    .entry(id.clone())
                    .or_insert_with(|| entry.path().to_path_buf());
            }
        }
    }
}

fn remember(base: &Path, id: &str, path: &Path) {
    if let Ok(mut cached) = path_cache().lock() {
        let key = (base.to_path_buf(), id.to_string());
        if cached.len() >= PATH_CACHE_LIMIT && !cached.contains_key(&key) {
            cached.clear();
        }
        cached.insert(key, path.to_path_buf());
    }
}
