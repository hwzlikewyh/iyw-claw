use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use sea_orm::DatabaseConnection;

use super::agent_storage::{AgentStorageConfig, AgentStoragePaths};
use crate::models::AgentType;

#[path = "xinghe_profile_data.rs"]
mod data;

const MARKER: &str = ".iyw-xinghe-migrated-v1";

/// 仅迁移应用托管目录，独立 Codex CLI 和用户自定义目录保持原样。
pub async fn migrate_legacy_codex_profile(
    paths: &AgentStoragePaths,
    config: &AgentStorageConfig,
    database: Option<&DatabaseConnection>,
) -> Result<bool, String> {
    let legacy = paths.config_dir().join("codex");
    let target = paths.config_dir().join("xinghe");
    let override_path = config
        .profile_overrides
        .get(super::registry::registry_id_for(AgentType::Codex));
    if override_path.is_some_and(|path| !same_path(path, &legacy) && !same_path(path, &target)) {
        return Ok(false);
    }
    if target.join(MARKER).is_file() {
        return Ok(false);
    }
    fs::create_dir_all(paths.config_dir()).map_err(|e| format!("prepare Xinghe parent: {e}"))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.config_dir().join(".xinghe-profile.lock"))
        .map_err(|e| format!("open Xinghe migration lock: {e}"))?;
    lock.try_lock()
        .map_err(|e| format!("Xinghe profile migration is busy: {e}"))?;
    if target.join(MARKER).is_file() {
        return Ok(false);
    }
    rename_profile(&legacy, &target)?;
    let roots = (legacy.as_path(), target.as_path());
    data::rebase_files(roots)?;
    data::rebase_databases(roots, database).await?;
    if let Some(database) = database {
        data::rebase_preferences(database, roots).await?;
    }
    write_atomic(&target.join(MARKER), b"migrated\n")?;
    tracing::info!("[agent-storage] Xinghe profile rename and path repair completed");
    Ok(true)
}

fn rename_profile(legacy: &Path, target: &Path) -> Result<(), String> {
    let old = directory_exists(legacy)?;
    let new = directory_exists(target)?;
    match (old, new) {
        (true, true) => Err(
            "Both config/codex and config/xinghe exist; preserved both profiles without merging"
                .into(),
        ),
        (true, false) => {
            fs::rename(legacy, target).map_err(|e| format!("rename Xinghe profile: {e}"))
        }
        (false, false) => {
            fs::create_dir_all(target).map_err(|e| format!("create Xinghe profile: {e}"))
        }
        (false, true) => Ok(()), // 上次若在改名后中断，继续修复引用，成功后才写完成标记。
    }
}

fn directory_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(true),
        Ok(_) => Err(format!(
            "Xinghe profile is not a regular directory: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect Xinghe profile: {error}")),
    }
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

pub(super) fn managed_override(paths: &AgentStoragePaths, path: &Path) -> PathBuf {
    if same_path(path, &paths.config_dir().join("codex")) {
        paths.config_dir().join("xinghe")
    } else {
        path.to_path_buf()
    }
}

fn normalized(path: &str) -> String {
    path.strip_prefix("\\\\?\\")
        .unwrap_or(path)
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = normalized(&left.to_string_lossy());
    let right = normalized(&right.to_string_lossy());
    left == right || (cfg!(windows) && left.eq_ignore_ascii_case(&right))
}

pub(super) fn rebase(value: &str, roots: (&Path, &Path)) -> Option<String> {
    let source = normalized(&roots.0.to_string_lossy());
    let value = normalized(value);
    let prefix = value.get(..source.len())?;
    if prefix != source && !(cfg!(windows) && prefix.eq_ignore_ascii_case(&source)) {
        return None;
    }
    let suffix = value.get(source.len()..)?;
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return None;
    }
    Some(
        roots
            .1
            .join(suffix.trim_start_matches('/'))
            .to_string_lossy()
            .into_owned(),
    )
}

pub(super) fn rebase_toml(raw: &str, roots: (&Path, &Path)) -> Result<Option<String>, String> {
    let mut value: toml::Value =
        toml::from_str(raw).map_err(|_| "Invalid Xinghe configuration TOML")?;
    fn visit(value: &mut toml::Value, roots: (&Path, &Path)) -> bool {
        match value {
            toml::Value::String(text) => match rebase(text, roots) {
                Some(next) if next != *text => {
                    *text = next;
                    true
                }
                _ => false,
            },
            toml::Value::Array(values) => values
                .iter_mut()
                .fold(false, |changed, value| visit(value, roots) || changed),
            toml::Value::Table(values) => values
                .iter_mut()
                .fold(false, |changed, (_, value)| visit(value, roots) || changed),
            _ => false,
        }
    }
    if visit(&mut value, roots) {
        toml::to_string(&value)
            .map(Some)
            .map_err(|_| "Cannot encode migrated Xinghe configuration".into())
    } else {
        Ok(None)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Xinghe file has no parent")?;
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("prepare Xinghe file: {e}"))?;
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| format!("write Xinghe file: {e}"))?;
    file.persist(path)
        .map_err(|e| format!("activate Xinghe file: {}", e.error))?;
    Ok(())
}
