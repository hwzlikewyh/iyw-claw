use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

pub fn safe_archive_path(value: &str) -> Result<PathBuf> {
    let value = value.replace('\\', "/");
    if value.starts_with('/') || value.contains(':') || value.contains('\0') {
        bail!("archive entry path is unsafe")
    }
    let mut path = PathBuf::new();
    for part in value.split('/').filter(|part| !matches!(*part, "" | ".")) {
        if part == ".." || part.ends_with([' ', '.']) {
            bail!("archive entry path is unsafe")
        }
        path.push(part);
    }
    Ok(path)
}

pub fn validate_link_target(root: &Path, link: &Path, target: &Path) -> Result<()> {
    if target.is_absolute() || target.to_string_lossy().contains([':', '\\', '\0']) {
        bail!("archive link target is unsafe")
    }
    let parent = link.parent().context("archive link has no parent")?;
    let mut depth = parent.strip_prefix(root)?.components().count();
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            _ => bail!("archive link target escapes the component root"),
        }
    }
    Ok(())
}

pub fn create_links(root: &Path, links: Vec<(PathBuf, PathBuf)>) -> Result<()> {
    // 先建所有父目录，再建链接，避免后续写入跟随此前创建的链接。
    for (link, _) in &links {
        fs::create_dir_all(link.parent().context("archive link has no parent")?)?;
    }
    for (link, target) in &links {
        create_link(target, link)?;
    }
    let canonical_root = root.canonicalize()?;
    for (link, _) in &links {
        if !link.canonicalize()?.starts_with(&canonical_root) {
            bail!("archive link resolves outside the component root")
        }
    }
    Ok(())
}

#[cfg(unix)]
fn create_link(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).context("create archive symbolic link")
}

#[cfg(windows)]
fn create_link(_target: &Path, _link: &Path) -> Result<()> {
    bail!("this Windows artifact unexpectedly contains symbolic links")
}
