use std::fs;
use std::io::Read;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

const MAX_LINK_TARGET_BYTES: u64 = 4096;
const EXECUTABLE_MODE: u32 = 0o755;

pub(super) fn make_executable(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(EXECUTABLE_MODE))
        .context("make macOS binary component executable")
}

pub(super) fn read_link(reader: &mut impl Read, root: &Path, link: &Path) -> Result<PathBuf> {
    let mut target = String::new();
    reader
        .take(MAX_LINK_TARGET_BYTES + 1)
        .read_to_string(&mut target)
        .context("read macOS ZIP symbolic link target")?;
    if target.is_empty() || target.len() as u64 > MAX_LINK_TARGET_BYTES {
        bail!("ZIP symbolic link target is empty or too long")
    }
    let target = PathBuf::from(target);
    super::validate_link_target(root, link, &target)?;
    Ok(target)
}

pub(super) fn create_links(root: &Path, links: Vec<(PathBuf, PathBuf)>) -> Result<()> {
    // 文件写完且所有父目录建好后再创建链接，避免解包写入跟随链接。
    for (link, _) in &links {
        super::create_parent(link)?;
    }
    for (link, target) in &links {
        symlink(target, link)
            .with_context(|| format!("create ZIP symbolic link: {}", link.display()))?;
    }
    let canonical_root = root.canonicalize()?;
    for (link, _) in &links {
        if !link.canonicalize()?.starts_with(&canonical_root) {
            bail!("ZIP symbolic link resolves outside the component root")
        }
    }
    Ok(())
}
