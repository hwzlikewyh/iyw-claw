use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path};

use crate::acp::error::AcpError;

const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_SINGLE_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(super) fn extract_runtime_bundle(
    archive_path: &Path,
    file_name: &str,
    destination: &Path,
) -> Result<(), AcpError> {
    if destination.exists() {
        std::fs::remove_dir_all(destination).map_err(io_error)?;
    }
    std::fs::create_dir_all(destination).map_err(io_error)?;
    let lower = file_name.to_ascii_lowercase();
    let result = if lower.ends_with(".zip") {
        extract_zip(archive_path, destination)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        extract_tar(archive_path, destination)
    } else {
        Err(AcpError::DownloadFailed(
            "Agent runtime bundle format is unsupported".into(),
        ))
    };
    if result.is_err() {
        let _ = std::fs::remove_dir_all(destination);
    }
    result
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), AcpError> {
    let file = File::open(archive_path).map_err(io_error)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| invalid_archive("Agent runtime ZIP is invalid", error))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(limit_error("entry count"));
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| invalid_archive("Agent runtime ZIP entry is invalid", error))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| unsafe_path_error())?
            .to_path_buf();
        reject_zip_link(&entry)?;
        let size = entry.size();
        expanded = checked_expanded(expanded, size)?;
        let output = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(output).map_err(io_error)?;
            continue;
        }
        create_parent(&output)?;
        let mut file = File::create(output).map_err(io_error)?;
        let written =
            std::io::copy(&mut entry.by_ref().take(size + 1), &mut file).map_err(io_error)?;
        if written != size {
            return Err(AcpError::DownloadFailed(
                "Agent runtime ZIP entry size is invalid".into(),
            ));
        }
        file.flush().map_err(io_error)?;
        apply_zip_permissions(&file, entry.unix_mode()).map_err(io_error)?;
    }
    Ok(())
}

#[cfg(unix)]
fn apply_zip_permissions(file: &File, mode: Option<u32>) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(mode) = mode {
        file.set_permissions(std::fs::Permissions::from_mode(mode & 0o777))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_zip_permissions(_file: &File, _mode: Option<u32>) -> Result<(), std::io::Error> {
    Ok(())
}

fn reject_zip_link(entry: &zip::read::ZipFile<'_>) -> Result<(), AcpError> {
    let file_type = entry.unix_mode().map(|mode| mode & 0o170000);
    if file_type.is_some_and(|kind| !matches!(kind, 0 | 0o040000 | 0o100000)) {
        return Err(AcpError::DownloadFailed(
            "Agent runtime ZIP contains an unsafe entry".into(),
        ));
    }
    Ok(())
}

fn extract_tar(archive_path: &Path, destination: &Path) -> Result<(), AcpError> {
    let file = File::open(archive_path).map_err(io_error)?;
    let reader = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|error| invalid_archive("Agent runtime TAR is invalid", error))?;
    let mut count = 0_usize;
    let mut expanded = 0_u64;
    for entry in entries {
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(limit_error("entry count"));
        }
        let mut entry =
            entry.map_err(|error| invalid_archive("Agent runtime TAR entry is invalid", error))?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(AcpError::DownloadFailed(
                "Agent runtime TAR contains an unsafe entry".into(),
            ));
        }
        let relative = entry
            .path()
            .map_err(|error| invalid_archive("Agent runtime TAR path is invalid", error))?;
        if !safe_relative_path(&relative) {
            return Err(unsafe_path_error());
        }
        let size = entry.header().size().map_err(io_error)?;
        expanded = checked_expanded(expanded, size)?;
        entry
            .unpack_in(destination)
            .map_err(|error| invalid_archive("Agent runtime TAR extraction failed", error))?;
    }
    Ok(())
}

fn checked_expanded(current: u64, size: u64) -> Result<u64, AcpError> {
    if size > MAX_SINGLE_FILE_BYTES {
        return Err(limit_error("single file size"));
    }
    let total = current
        .checked_add(size)
        .ok_or_else(|| limit_error("expanded size"))?;
    (total <= MAX_EXPANDED_BYTES)
        .then_some(total)
        .ok_or_else(|| limit_error("expanded size"))
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|item| matches!(item, Component::Normal(_)))
}

fn create_parent(path: &Path) -> Result<(), AcpError> {
    let parent = path
        .parent()
        .ok_or_else(|| AcpError::DownloadFailed("Agent runtime entry has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(io_error)
}

fn io_error(error: std::io::Error) -> AcpError {
    AcpError::DownloadFailed(error.to_string())
}

fn invalid_archive(context: &str, error: impl std::fmt::Display) -> AcpError {
    AcpError::DownloadFailed(format!("{context}: {error}"))
}

fn unsafe_path_error() -> AcpError {
    AcpError::DownloadFailed("Agent runtime archive contains an unsafe path".into())
}

fn limit_error(kind: &str) -> AcpError {
    AcpError::DownloadFailed(format!("Agent runtime archive exceeds the {kind} limit"))
}
