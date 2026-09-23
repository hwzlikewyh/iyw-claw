use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use tar::EntryType;
use xz2::read::XzDecoder;
use zip::ZipArchive;

const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub fn unpack(archive: &Path, destination: &Path, kind: &str, file_name: &str) -> Result<()> {
    create_directory(destination)?;
    match kind {
        "binary" => {
            validate_binary_name(file_name)?;
            let output = destination.join(file_name);
            let size = fs::metadata(archive)
                .with_context(|| format!("read binary artifact size: {}", archive.display()))?
                .len();
            crate::paths::require_space(destination, size)?;
            crate::retry::file("stage binary component", &output, || {
                fs::copy(archive, &output)
            })?;
            Ok(())
        }
        "zip" | "npm_runtime_bundle_zip" | "uvx_runtime_bundle_zip" => {
            unpack_zip(archive, destination)
        }
        "tar_gz" | "npm_runtime_bundle_tar_gz" | "uvx_runtime_bundle_tar_gz" => {
            unpack_tar(GzDecoder::new(open_archive(archive)?), destination)
        }
        "tar_xz" => unpack_tar(XzDecoder::new(open_archive(archive)?), destination),
        _ => bail!("unsupported environment package kind: {kind}"),
    }
}

fn validate_binary_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 255
        || matches!(value, "." | "..")
        || value.contains(['/', '\\', ':'])
    {
        bail!("binary artifact file name is invalid")
    }
    Ok(())
}

fn unpack_zip(archive: &Path, destination: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(open_archive(archive)?).context("open ZIP artifact")?;
    check_zip_space(&mut archive, destination)?;
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read ZIP entry")?;
        let enclosed = entry.enclosed_name().context("ZIP entry path is unsafe")?;
        reject_unsafe_path(&enclosed)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            bail!("ZIP symbolic links are not allowed")
        }
        let output = destination.join(enclosed);
        if entry.is_dir() {
            create_directory(&output)?;
            continue;
        }
        expanded = checked_expanded(expanded, entry.size())?;
        write_entry(&output, &mut entry)?;
        apply_mode(&output, entry.unix_mode())?;
    }
    Ok(())
}

fn unpack_tar<R: Read>(reader: R, destination: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut expanded = 0_u64;
    let mut links = Vec::new();
    for item in archive.entries().context("read TAR entries")? {
        let mut entry = item.context("read TAR entry")?;
        let kind = entry.header().entry_type();
        if kind != EntryType::Regular && kind != EntryType::Directory && kind != EntryType::Symlink
        {
            bail!("TAR links and special entries are not allowed")
        }
        let relative = entry.path().context("read TAR path")?.into_owned();
        reject_unsafe_path(&relative)?;
        let output = destination.join(relative);
        if kind == EntryType::Directory {
            create_directory(&output)?;
            continue;
        }
        if kind == EntryType::Symlink {
            let target = entry
                .link_name()?
                .context("TAR link target is missing")?
                .into_owned();
            validate_link_target(destination, &output, &target)?;
            links.push((output, target));
            continue;
        }
        expanded = checked_expanded(expanded, entry.size())?;
        crate::paths::require_space(destination, entry.size())?;
        let mode = entry.header().mode().ok();
        write_entry(&output, &mut entry)?;
        apply_mode(&output, mode)?;
    }
    create_links(links)?;
    Ok(())
}

fn open_archive(path: &Path) -> Result<File> {
    crate::retry::file("open component archive", path, || File::open(path))
}

fn create_directory(path: &Path) -> Result<()> {
    crate::retry::file("create archive directory", path, || {
        fs::create_dir_all(path)
    })
}

fn write_entry(path: &Path, entry: &mut impl Read) -> Result<()> {
    create_parent(path)?;
    let mut target = crate::retry::file("create archive file", path, || File::create(path))?;
    io::copy(entry, &mut target)
        .with_context(|| format!("write archive file: {}", path.display()))?;
    Ok(())
}

fn check_zip_space(archive: &mut ZipArchive<File>, destination: &Path) -> Result<()> {
    let mut expanded = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).context("read ZIP entry size")?;
        expanded = checked_expanded(expanded, entry.size())?;
    }
    crate::paths::require_space(destination, expanded)
}

fn validate_link_target(root: &Path, link: &Path, target: &Path) -> Result<()> {
    if target.is_absolute() {
        bail!("TAR link target is absolute")
    }
    let parent = link.parent().context("TAR link has no parent")?;
    let mut depth = parent.strip_prefix(root)?.components().count();
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            _ => bail!("TAR link target escapes the component root"),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn create_links(links: Vec<(std::path::PathBuf, std::path::PathBuf)>) -> Result<()> {
    use std::os::unix::fs::symlink;
    for (link, target) in links {
        create_parent(&link)?;
        symlink(target, link).context("create TAR symbolic link")?;
    }
    Ok(())
}

#[cfg(windows)]
fn create_links(links: Vec<(std::path::PathBuf, std::path::PathBuf)>) -> Result<()> {
    if links.is_empty() {
        Ok(())
    } else {
        bail!("TAR symbolic links are not supported on Windows")
    }
}

fn reject_unsafe_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || path.to_string_lossy().contains(':')
    {
        bail!("archive entry path is unsafe")
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("archive entry has no parent")?;
    create_directory(parent)
}

fn checked_expanded(current: u64, additional: u64) -> Result<u64> {
    let total = current
        .checked_add(additional)
        .context("expanded size overflow")?;
    if total > MAX_EXPANDED_BYTES {
        bail!("expanded environment artifact is too large")
    }
    Ok(total)
}

#[cfg(unix)]
fn apply_mode(path: &Path, mode: Option<u32>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(mode) = mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))?;
    }
    Ok(())
}

#[cfg(windows)]
fn apply_mode(_path: &Path, _mode: Option<u32>) -> Result<()> {
    Ok(())
}
