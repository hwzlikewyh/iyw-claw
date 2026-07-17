use std::fs;
use std::path::Path;

use include_dir::{Dir, DirEntry};
use sha2::{Digest, Sha256};

use super::metadata::bundled_skill_dir;
use super::{ScienceError, ScienceMetadata};

pub(super) fn hash_disk_directory(path: &Path) -> Result<String, ScienceError> {
    let mut files = Vec::new();
    collect_disk_files(path, path, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let skill_id = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    for (relative, contents) in files {
        hasher.update(format!("skills/{skill_id}/{relative}").as_bytes());
        hasher.update(b"\0");
        hasher.update(contents);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_disk_files(
    base: &Path,
    current: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), ScienceError> {
    if !current.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_disk_files(base, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(base)
                .map_err(|error| ScienceError::Io(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, fs::read(path)?));
        }
    }
    Ok(())
}

pub(super) fn extract_skill(metadata: &ScienceMetadata, target: &Path) -> Result<(), ScienceError> {
    let directory = bundled_skill_dir(&metadata.id)?;
    fs::create_dir_all(target)?;
    extract_directory(directory, &format!("skills/{}", metadata.id), target)
}

fn extract_directory(
    directory: &Dir<'_>,
    bundle_prefix: &str,
    target: &Path,
) -> Result<(), ScienceError> {
    for entry in directory.entries() {
        match entry {
            DirEntry::File(file) => extract_file(file, bundle_prefix, target)?,
            DirEntry::Dir(child) => extract_directory(child, bundle_prefix, target)?,
        }
    }
    Ok(())
}

fn extract_file(
    file: &include_dir::File<'_>,
    bundle_prefix: &str,
    target: &Path,
) -> Result<(), ScienceError> {
    let relative = file
        .path()
        .to_str()
        .ok_or_else(|| ScienceError::Io("non-UTF-8 path in science bundle".to_string()))?
        .strip_prefix(bundle_prefix)
        .and_then(|path| path.strip_prefix('/'))
        .unwrap_or_default();
    let output = target.join(relative);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, file.contents())?;
    restore_executable_bit(&output, file.contents())?;
    Ok(())
}

#[cfg(unix)]
fn restore_executable_bit(path: &Path, contents: &[u8]) -> Result<(), ScienceError> {
    use std::os::unix::fs::PermissionsExt;

    if contents.starts_with(b"#!") {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restore_executable_bit(_path: &Path, _contents: &[u8]) -> Result<(), ScienceError> {
    Ok(())
}
