use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::runtime_seed_manifest::{RuntimeSeedComponent, RuntimeSeedFile};
use crate::app_error::AppCommandError;

pub(super) fn stage_component(
    seed_root: &Path,
    component: &RuntimeSeedComponent,
    destination: &Path,
) -> Result<(), AppCommandError> {
    let archive = component.source_archive(seed_root);
    validate_archive(seed_root, &archive, component)?;
    std::fs::create_dir_all(destination).map_err(AppCommandError::io)?;
    extract_component(&archive, component, destination)
}

fn validate_archive(
    seed_root: &Path,
    archive: &Path,
    component: &RuntimeSeedComponent,
) -> Result<(), AppCommandError> {
    let metadata = std::fs::symlink_metadata(archive).map_err(AppCommandError::io)?;
    if !metadata.file_type().is_file() || metadata.len() != component.archive_size {
        return Err(invalid("Runtime seed archive size or type is invalid"));
    }
    let canonical_seed = std::fs::canonicalize(seed_root).map_err(AppCommandError::io)?;
    let canonical_archive = std::fs::canonicalize(archive).map_err(AppCommandError::io)?;
    if !canonical_archive.starts_with(canonical_seed) {
        return Err(invalid("Runtime seed archive escaped its resource root"));
    }
    let actual = hash_reader(File::open(archive).map_err(AppCommandError::io)?)?;
    if !actual.eq_ignore_ascii_case(&component.archive_sha256) {
        return Err(invalid("Runtime seed archive SHA-256 mismatch"));
    }
    Ok(())
}

fn extract_component(
    archive_path: &Path,
    component: &RuntimeSeedComponent,
    destination: &Path,
) -> Result<(), AppCommandError> {
    let file = File::open(archive_path).map_err(AppCommandError::io)?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let mut expected = expected_files(component);
    let allowed_dirs = allowed_directories(component);
    let mut seen = BTreeSet::new();
    let entries = archive.entries().map_err(archive_error)?;
    for entry in entries {
        let mut entry = entry.map_err(archive_error)?;
        let raw_path = entry.path().map_err(archive_error)?;
        let Some(path) = normalize_archive_path(&raw_path)? else {
            if entry.header().entry_type().is_dir() {
                continue;
            }
            return Err(invalid("Runtime seed archive contains an empty file path"));
        };
        if !seen.insert(path.clone()) {
            return Err(invalid("Runtime seed archive contains duplicate paths"));
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            if !allowed_dirs.contains(&path) {
                return Err(invalid(
                    "Runtime seed archive contains an unlisted directory",
                ));
            }
            std::fs::create_dir_all(destination.join(path)).map_err(AppCommandError::io)?;
            continue;
        }
        if !entry_type.is_file() {
            return Err(invalid(
                "Runtime seed archive contains a link or unsupported entry",
            ));
        }
        let declared = expected
            .remove(&path)
            .ok_or_else(|| invalid("Runtime seed archive contains an unlisted file"))?;
        extract_file(&mut entry, destination, &path, declared)?;
    }
    if !expected.is_empty() {
        return Err(invalid("Runtime seed archive is missing declared files"));
    }
    Ok(())
}

fn expected_files(component: &RuntimeSeedComponent) -> BTreeMap<PathBuf, &RuntimeSeedFile> {
    component
        .files
        .iter()
        .map(|file| (PathBuf::from(&file.path), file))
        .collect()
}

fn allowed_directories(component: &RuntimeSeedComponent) -> BTreeSet<PathBuf> {
    let mut directories = BTreeSet::new();
    for file in &component.files {
        let mut parent = Path::new(&file.path).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    directories
}

fn normalize_archive_path(path: &Path) -> Result<Option<PathBuf>, AppCommandError> {
    let raw = path.to_string_lossy().replace('\\', "/");
    let path = Path::new(&raw);
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) if !value.to_string_lossy().chars().any(char::is_control) => {
                normalized.push(value)
            }
            _ => return Err(invalid("Runtime seed archive contains an unsafe path")),
        }
    }
    Ok((!normalized.as_os_str().is_empty()).then_some(normalized))
}

fn extract_file<R: Read>(
    entry: &mut R,
    destination: &Path,
    path: &Path,
    declared: &RuntimeSeedFile,
) -> Result<(), AppCommandError> {
    let target = destination.join(path);
    let parent = target
        .parent()
        .ok_or_else(|| invalid("Runtime seed file has no destination parent"))?;
    std::fs::create_dir_all(parent).map_err(AppCommandError::io)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(AppCommandError::io)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = entry.read(&mut buffer).map_err(AppCommandError::io)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| invalid("Runtime seed file size overflow"))?;
        if total > declared.size {
            return Err(invalid("Runtime seed file exceeds its declared size"));
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(AppCommandError::io)?;
    }
    output.flush().map_err(AppCommandError::io)?;
    let actual = format!("{:x}", hasher.finalize());
    if total != declared.size || !actual.eq_ignore_ascii_case(&declared.sha256) {
        return Err(invalid("Runtime seed file size or SHA-256 mismatch"));
    }
    set_executable(&target, declared.executable)
}

fn hash_reader<R: Read>(mut reader: R) -> Result<String, AppCommandError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(AppCommandError::io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn archive_error(error: impl std::fmt::Display) -> AppCommandError {
    invalid("Runtime seed archive is invalid").with_detail(error.to_string())
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::invalid_input(message)
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<(), AppCommandError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if executable { 0o755 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(AppCommandError::io)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<(), AppCommandError> {
    Ok(())
}
