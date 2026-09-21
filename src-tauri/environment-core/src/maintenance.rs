use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::paths::{ensure_within, safe_segment, Layout};

pub fn install(layout: &Layout, app_version: &str) -> Result<()> {
    safe_segment(app_version, "application version")?;
    let source = std::env::current_exe()?;
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
