use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::PreparedState;
use crate::paths::{ensure_within, safe_segment, Layout};

pub fn install(layout: &Layout, state: &PreparedState) -> Result<()> {
    safe_segment(&state.pc_version, "application version")?;
    let source = source_executable(layout, state)?;
    let digest = crate::download::hash_file(&source)?;
    let (_, _, platform) = crate::paths::platform();
    let relative = format!("{}/{platform}/{digest}", env!("CARGO_PKG_VERSION"));
    let root = layout.root.join("maintenance");
    ensure_within(&layout.root, &root)?;
    let directory = root.join(&relative);
    fs::create_dir_all(&directory)?;
    let name = format!("iyw-environment{}", std::env::consts::EXE_SUFFIX);
    let target = directory.join(&name);
    if !crate::download::valid_file(&target, source.metadata()?.len(), &digest)? {
        fs::copy(&source, &target).context("安装独立环境修复程序失败")?;
        if crate::download::hash_file(&target)? != digest {
            anyhow::bail!("环境修复程序复制校验失败")
        }
    }
    write_launcher(&root, &format!("{relative}/{name}"))
}

fn source_executable(layout: &Layout, state: &PreparedState) -> Result<std::path::PathBuf> {
    let Some(prepared) = state
        .components
        .iter()
        .find(|entry| entry.component.component_id == "environment-maintainer")
    else {
        return std::env::current_exe().context("安装引导程序路径不可用");
    };
    let component = &prepared.component;
    let installed = crate::paths::from_slash(&layout.root, &component.relative_path)?;
    let staged = layout
        .transaction_dir(&state.transaction_id)?
        .join("components/environment-maintainer");
    let root = if prepared.staged && staged.exists() {
        staged
    } else {
        installed
    };
    let entry = component
        .entrypoints
        .get("environment-maintainer")
        .context("环境修复程序缺少入口")?;
    let record = component
        .files
        .iter()
        .find(|record| record.path == *entry)
        .context("环境修复程序入口缺少校验记录")?;
    let source = crate::paths::from_slash(&root, entry)?;
    ensure_within(&layout.root, &source)?;
    if !source.canonicalize()?.starts_with(root.canonicalize()?)
        || !crate::download::valid_file(&source, record.size, &record.sha256)?
    {
        anyhow::bail!("环境修复程序入口校验失败");
    }
    Ok(source)
}

fn write_launcher(root: &Path, relative: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let executable = relative.replace('/', "\\");
        let text = format!("@echo off\r\n\"%~dp0{executable}\" repair\r\npause\r\n");
        fs::write(root.join("repair.cmd"), text)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let text = format!("#!/bin/sh\nset -eu\nbase=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nexec \"$base/{relative}\" repair\n");
        let target = root.join("repair.sh");
        fs::write(&target, text)?;
        fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
