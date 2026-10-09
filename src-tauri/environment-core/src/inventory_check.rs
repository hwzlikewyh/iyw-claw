use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::download::hash_file;
use crate::model::{FileRecord, InstalledComponent};
use crate::paths::{from_slash, Layout};

pub(super) fn component(
    layout: &Layout,
    component: &InstalledComponent,
    full_check: bool,
) -> Result<()> {
    let root = from_slash(&layout.root, &component.relative_path)?;
    if !root.is_dir() || component.files.is_empty() || component.entrypoints.is_empty() {
        bail!("component directory or installation record is missing")
    }
    super::validate_entrypoints(&component.component_id, &component.entrypoints)?;
    if full_check {
        for record in super::cache::immutable_records(&component.component_id, &component.files) {
            check_file(&root, record, true)?;
        }
    }
    check_entrypoints(&root, component, full_check)
}

fn check_entrypoints(root: &Path, component: &InstalledComponent, full_check: bool) -> Result<()> {
    for (name, relative) in &component.entrypoints {
        // 只查内存中的安装记录，不扫描目录，也不读取已有文件内容。
        let record = component
            .files
            .iter()
            .find(|record| record.path == *relative)
            .with_context(|| format!("component entrypoint record is missing: {name}"))?;
        if !full_check {
            check_file(root, record, false)?;
        }
        if !from_slash(root, relative)?.is_file() {
            bail!("installed component entrypoint is missing: {name}")
        }
    }
    Ok(())
}

fn check_file(root: &Path, record: &FileRecord, full_check: bool) -> Result<()> {
    let path = from_slash(root, &record.path)?;
    if let Some(expected) = &record.link_target {
        let metadata = fs::symlink_metadata(&path).context("read component link")?;
        let actual = fs::read_link(&path).context("read component link target")?;
        if !metadata.file_type().is_symlink()
            || actual.to_string_lossy().replace('\\', "/") != *expected
        {
            bail!(
                "installed component link failed verification: {}",
                record.path
            )
        }
        return Ok(());
    }
    let metadata = fs::metadata(&path)
        .with_context(|| format!("read installed component file: {}", record.path))?;
    if !metadata.is_file()
        || metadata.len() != record.size
        || (full_check && hash_file(&path)? != record.sha256)
    {
        bail!(
            "installed component file failed verification: {}",
            record.path
        )
    }
    Ok(())
}
