use std::io::{Cursor, Read};
use std::path::Path;

use crate::app_error::AppCommandError;

const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_SINGLE_FILE_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn extract_archive(
    bytes: &[u8],
    file_name: &str,
    destination: &Path,
) -> Result<(), AppCommandError> {
    std::fs::create_dir_all(destination).map_err(AppCommandError::io)?;
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        return extract_zip(bytes, destination);
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return extract_tar(
            flate2::read::GzDecoder::new(Cursor::new(bytes)),
            destination,
        );
    }
    if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        return extract_tar(bzip2::read::BzDecoder::new(Cursor::new(bytes)), destination);
    }
    Err(AppCommandError::invalid_input(
        "Agent archive format is unsupported",
    ))
}

fn extract_zip(bytes: &[u8], destination: &Path) -> Result<(), AppCommandError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        AppCommandError::invalid_input("Agent artifact is not a ZIP").with_detail(error.to_string())
    })?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(AppCommandError::invalid_input(
            "Agent archive has too many entries",
        ));
    }
    let mut extracted = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            AppCommandError::invalid_input("Agent archive entry is unreadable")
                .with_detail(error.to_string())
        })?;
        extracted += extract_zip_entry(&mut entry, destination, extracted)?;
    }
    Ok(())
}

fn extract_zip_entry(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &Path,
    extracted: u64,
) -> Result<u64, AppCommandError> {
    let relative = entry
        .enclosed_name()
        .ok_or_else(|| AppCommandError::invalid_input("Agent archive contains an unsafe path"))?;
    if has_unsafe_zip_entry_type(entry) {
        return Err(AppCommandError::invalid_input(
            "Agent ZIP contains an unsafe entry",
        ));
    }
    if entry.size() > MAX_SINGLE_FILE_BYTES {
        return Err(AppCommandError::invalid_input(
            "Agent archive contains an oversized file",
        ));
    }
    let output = destination.join(relative);
    if entry.is_dir() {
        std::fs::create_dir_all(output).map_err(AppCommandError::io)?;
        return Ok(0);
    }
    let parent = output
        .parent()
        .ok_or_else(|| AppCommandError::invalid_input("Agent archive entry has no parent"))?;
    std::fs::create_dir_all(parent).map_err(AppCommandError::io)?;
    let remaining = MAX_EXTRACTED_BYTES.saturating_sub(extracted);
    let mut file = std::fs::File::create(output).map_err(AppCommandError::io)?;
    let written = std::io::copy(&mut entry.by_ref().take(remaining + 1), &mut file)
        .map_err(AppCommandError::io)?;
    (written <= remaining)
        .then_some(written)
        .ok_or_else(|| AppCommandError::invalid_input("Agent archive expands beyond the limit"))
}

fn has_unsafe_zip_entry_type(entry: &zip::read::ZipFile<'_>) -> bool {
    entry
        .unix_mode()
        .map(|mode| mode & 0o170000)
        .is_some_and(|kind| !matches!(kind, 0 | 0o040000 | 0o100000))
}

fn extract_tar<R: Read>(reader: R, destination: &Path) -> Result<(), AppCommandError> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive.entries().map_err(|error| {
        AppCommandError::invalid_input("Agent TAR is unreadable").with_detail(error.to_string())
    })?;
    let mut count = 0_usize;
    let mut extracted = 0_u64;
    for entry in entries {
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(AppCommandError::invalid_input(
                "Agent archive has too many entries",
            ));
        }
        extracted += extract_tar_entry(entry, destination, extracted)?;
    }
    Ok(())
}

fn extract_tar_entry<R: Read>(
    entry: Result<tar::Entry<'_, R>, std::io::Error>,
    destination: &Path,
    extracted: u64,
) -> Result<u64, AppCommandError> {
    let mut entry = entry.map_err(|error| {
        AppCommandError::invalid_input("Agent TAR entry is unreadable")
            .with_detail(error.to_string())
    })?;
    let relative = entry.path().map_err(|error| {
        AppCommandError::invalid_input("Agent TAR path is invalid").with_detail(error.to_string())
    })?;
    if relative.is_absolute()
        || relative
            .components()
            .any(|item| matches!(item, std::path::Component::ParentDir))
    {
        return Err(AppCommandError::invalid_input(
            "Agent TAR contains an unsafe path",
        ));
    }
    let entry_type = entry.header().entry_type();
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        return Ok(0);
    }
    if !entry_type.is_file() && !entry_type.is_dir() {
        return Err(AppCommandError::invalid_input(
            "Agent TAR contains an unsafe entry",
        ));
    }
    let size = entry.header().size().unwrap_or(u64::MAX);
    if size > MAX_SINGLE_FILE_BYTES || extracted.saturating_add(size) > MAX_EXTRACTED_BYTES {
        return Err(AppCommandError::invalid_input(
            "Agent TAR expands beyond the limit",
        ));
    }
    entry.unpack_in(destination).map_err(|error| {
        AppCommandError::invalid_input("Agent TAR extraction failed").with_detail(error.to_string())
    })?;
    Ok(size)
}
