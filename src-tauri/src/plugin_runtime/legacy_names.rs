use crate::app_error::AppCommandError;
use std::path::{Path, PathBuf};

use crate::models::plugin_names::migration;
pub(crate) use crate::models::plugin_names::{alternate_name, same_capability, same_plugin};

const MAX_POINTER_BYTES: u64 = 128 * 1024;

pub(crate) fn require_owner(id: i64, slug: &str) -> Result<(), AppCommandError> {
    if migration(slug).is_some_and(|value| value.id != id) {
        return Err(invalid("插件兼容名称属于其他市场条目"));
    }
    Ok(())
}

// 升级复用可信旧目录，避免移动用户数据与破坏历史版本收据。
pub(crate) fn package_root(base: &Path, slug: &str) -> Result<PathBuf, AppCommandError> {
    let selected = select_directory(base, slug)?;
    if let Some(value) = migration(slug).filter(|_| selected.exists()) {
        let file = selected.join("current.json");
        let metadata = std::fs::symlink_metadata(&file)
            .map_err(|_| invalid("改名插件缺少安装指针，请先修复安装"))?;
        if !metadata.is_file() || is_link(&metadata) || metadata.len() > MAX_POINTER_BYTES {
            return Err(invalid("插件改名安装指针无效"));
        }
        let pointer: super::recovery::CurrentPointer = serde_json::from_slice(
            &std::fs::read(file).map_err(|_| invalid("插件改名安装指针无法读取"))?,
        )
        .map_err(|_| invalid("插件改名安装指针损坏"))?;
        if !pointer.market.is_some_and(|marker| {
            marker.skill_id == value.id
                && marker.source == "iyw_skill_market"
                && same_plugin(value.id, &marker.slug, slug)
        }) {
            return Err(invalid("插件目录与已授权迁移条目不符"));
        }
    }
    Ok(selected)
}

pub(crate) fn data_directory(base: &Path, slug: &str) -> Result<PathBuf, AppCommandError> {
    select_directory(base, slug)
}

fn select_directory(base: &Path, slug: &str) -> Result<PathBuf, AppCommandError> {
    let Some(value) = migration(slug) else {
        return Ok(base.join(slug));
    };
    let legacy = base.join(value.legacy);
    let current = base.join(value.name);
    let old_exists = real_directory(&legacy)?;
    let new_exists = real_directory(&current)?;
    match (old_exists, new_exists) {
        (true, true) => Err(invalid("插件新旧目录同时存在，请先修复安装，原数据已保留")),
        (true, false) => Ok(legacy),
        (false, true) => Ok(current),
        (false, false) => Ok(base.join(slug)),
    }
}

fn real_directory(path: &Path) -> Result<bool, AppCommandError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(true),
        Ok(_) => Err(invalid("插件兼容路径必须是实体目录")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(invalid("插件兼容目录无法读取")),
    }
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
