use std::fs::File;
use std::path::{Path, PathBuf};

use super::{verification_cache, ManagedFile};

pub(super) fn verify(root: &Path, path: &Path, record: &ManagedFile) -> Result<(), String> {
    let canonical = canonical_file(root, path)?;
    if let Some(expected) = record.link_target.as_deref() {
        return verify_link(path, expected);
    }
    let mut file = File::open(&canonical).map_err(|error| format!("无法读取组件文件：{error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("无法读取组件文件信息：{error}"))?;
    if !metadata.is_file() || metadata.len() != record.size {
        return Err("组件文件大小与安装记录不符".into());
    }
    verification_cache::verify(&mut file, &canonical, record)
}

fn canonical_file(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("组件目录不可用：{error}"))?;
    let path = path
        .canonicalize()
        .map_err(|error| format!("组件入口文件不可用：{error}"))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("组件入口不是受管目录内的文件".into());
    }
    Ok(path)
}

fn verify_link(path: &Path, expected: &str) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| format!("无法读取组件链接：{error}"))?;
    let target =
        std::fs::read_link(path).map_err(|error| format!("无法读取组件链接目标：{error}"))?;
    if !metadata.file_type().is_symlink() || target.to_string_lossy().replace('\\', "/") != expected
    {
        return Err("组件链接目标与安装记录不符".into());
    }
    Ok(())
}
