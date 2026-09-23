use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::file_stamp::FileStamp;
use super::Snapshot;

struct CachedSnapshot {
    path: PathBuf,
    stamp: FileStamp,
    snapshot: Arc<Snapshot>,
}

static CACHE: OnceLock<Mutex<Option<CachedSnapshot>>> = OnceLock::new();

pub(super) fn clear() {
    if let Some(cache) = CACHE.get() {
        *cache.lock().unwrap_or_else(|error| error.into_inner()) = None;
    }
}

pub(super) fn load(root: &Path) -> Result<Arc<Snapshot>, String> {
    let path = root.join("inventory/environment-current.json");
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (snapshot, stamp) = match read_snapshot(&path, cache.as_ref()) {
        Ok(value) => value,
        Err(error) => {
            *cache = None;
            return Err(error);
        }
    };
    let ids = snapshot
        .components
        .iter()
        .map(|item| item.component_id.as_str())
        .collect();
    if let Err(error) = validate_selection(root, &snapshot, &ids) {
        *cache = None;
        return Err(error);
    }
    *cache = stamp.map(|stamp| CachedSnapshot {
        path,
        stamp,
        snapshot: Arc::clone(&snapshot),
    });
    Ok(snapshot)
}

fn read_snapshot(
    path: &Path,
    cached: Option<&CachedSnapshot>,
) -> Result<(Arc<Snapshot>, Option<FileStamp>), String> {
    let mut file =
        File::open(path).map_err(|_| "环境清单缺失或不可读，请运行环境修复".to_string())?;
    let before = FileStamp::read(&file);
    let snapshot = match cached
        .filter(|entry| entry.path.as_path() == path && before.as_ref() == Some(&entry.stamp))
    {
        Some(entry) => Arc::clone(&entry.snapshot),
        None => {
            let mut raw = Vec::new();
            file.read_to_end(&mut raw)
                .map_err(|_| "环境清单缺失或不可读，请运行环境修复".to_string())?;
            Arc::new(parse(&raw)?)
        }
    };
    let current =
        File::open(path).map_err(|_| "环境清单缺失或不可读，请运行环境修复".to_string())?;
    if before != FileStamp::read(&file) || before != FileStamp::read(&current) {
        return Err("环境清单在读取期间发生变化，请重试".into());
    }
    Ok((snapshot, before))
}

fn parse(raw: &[u8]) -> Result<Snapshot, String> {
    let snapshot: Snapshot =
        serde_json::from_slice(raw).map_err(|_| "环境清单格式错误，请运行环境修复".to_string())?;
    let target = match std::env::consts::OS {
        "macos" => "darwin",
        value => value,
    };
    if snapshot.schema_version != 1
        || snapshot.pc_version != env!("CARGO_PKG_VERSION")
        || snapshot.target != target
        || snapshot.arch != std::env::consts::ARCH
        || snapshot.catalog_revision == 0
    {
        return Err("环境与当前应用版本或系统架构不匹配，请运行环境修复".into());
    }
    let mut ids = BTreeSet::new();
    if snapshot.components.iter().any(|component| {
        !ids.insert(component.component_id.as_str())
            || super::status::required_entrypoints(&component.component_id).is_empty()
    }) {
        return Err("环境清单包含重复或未知组件，请运行环境修复".into());
    }
    Ok(snapshot)
}

fn validate_selection(
    root: &Path,
    snapshot: &Snapshot,
    ids: &BTreeSet<&str>,
) -> Result<(), String> {
    let Some(selected) = &snapshot.selected_components else {
        if ["node", "git", "uv", "chromix", "agent-browser"]
            .iter()
            .all(|id| ids.contains(id))
        {
            return Ok(());
        }
        return Err("环境清单缺少组件或安装策略，请运行环境修复".into());
    };
    if selected.len() != ids.len()
        || selected.iter().map(String::as_str).collect::<BTreeSet<_>>() != *ids
    {
        return Err("环境清单与安装时选定的组件不一致，请运行环境修复".into());
    }
    let receipt = super::managed_path(
        &root.join("inventory/transactions"),
        &format!("{}.json", snapshot.generation),
    )
    .ok_or("环境事务标识无效，请运行环境修复")?;
    let bytes = std::fs::read(receipt).map_err(|_| "环境提交尚未完成，请运行环境修复")?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "环境事务记录异常")?;
    if value["generation"] != snapshot.generation || value["state"] != "committed" {
        return Err("环境事务记录异常，请运行环境修复".into());
    }
    Ok(())
}

pub(super) fn component_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = super::managed_path(root, relative)?;
    let runtime = root.join("runtime").canonicalize().ok()?;
    let canonical = path.canonicalize().ok()?;
    (canonical.starts_with(runtime) && canonical.is_dir()).then_some(path)
}
