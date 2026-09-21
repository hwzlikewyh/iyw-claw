use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use tar::EntryType;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::archive_links::{create_links, safe_archive_path, validate_link_target};

const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub struct UnpackRequest<'a> {
    pub archive: &'a Path,
    pub destination: &'a Path,
    pub kind: &'a str,
    pub file_name: &'a str,
}

pub fn unpack(request: UnpackRequest<'_>) -> Result<()> {
    fs::create_dir_all(request.destination).context("create component staging directory")?;
    match request.kind {
        "binary" => {
            validate_binary_name(request.file_name)?;
            let path = request.destination.join(request.file_name);
            fs::copy(request.archive, &path).context("stage binary component")?;
            apply_mode(&path, Some(0o755))?;
            Ok(())
        }
        "zip" | "npm_runtime_bundle_zip" | "uvx_runtime_bundle_zip" => {
            unpack_zip(request.archive, request.destination)
        }
        "tar_gz" | "npm_runtime_bundle_tar_gz" | "uvx_runtime_bundle_tar_gz" => unpack_tar(
            GzDecoder::new(File::open(request.archive)?),
            request.destination,
        ),
        "tar_xz" => unpack_tar(
            XzDecoder::new(File::open(request.archive)?),
            request.destination,
        ),
        _ => bail!("unsupported environment package kind: {}", request.kind),
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
    let mut archive = ZipArchive::new(File::open(archive)?).context("open ZIP artifact")?;
    let mut expanded = 0_u64;
    let mut links = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read ZIP entry")?;
        let enclosed = safe_archive_path(entry.name())?;
        let output = destination.join(enclosed);
        expanded = checked_expanded(expanded, entry.size())?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            let mut target = String::new();
            entry.take(4097).read_to_string(&mut target)?;
            if target.len() > 4096 {
                bail!("archive link target is too long")
            }
            let target = std::path::PathBuf::from(target);
            validate_link_target(destination, &output, &target)?;
            links.push((output, target));
            continue;
        }
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        create_parent(&output)?;
        let mut target = File::create_new(&output)?;
        io::copy(&mut entry, &mut target).context("extract ZIP entry")?;
        apply_mode(&output, entry.unix_mode())?;
    }
    create_links(destination, links)
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
        let relative = safe_archive_path(&entry.path()?.to_string_lossy())?;
        let output = destination.join(relative);
        if kind == EntryType::Directory {
            fs::create_dir_all(output)?;
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
        let mode = entry.header().mode().ok();
        create_parent(&output)?;
        let mut target = File::create_new(&output)?;
        io::copy(&mut entry, &mut target).context("extract TAR entry")?;
        apply_mode(&output, mode)?;
    }
    create_links(destination, links)?;
    Ok(())
}

fn create_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("archive entry has no parent")?;
    fs::create_dir_all(parent).context("create archive entry parent")
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
