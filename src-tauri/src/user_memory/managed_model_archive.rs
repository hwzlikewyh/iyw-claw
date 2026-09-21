use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::managed_model_types::{model_error, ModelManifest, MANIFEST_FILE, MODEL_FILES};
use crate::app_error::AppCommandError;

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

pub(super) fn extract_archive(
    archive_path: &Path,
    destination: &Path,
) -> Result<ModelManifest, AppCommandError> {
    let file = File::open(archive_path).map_err(AppCommandError::io)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| model_error(format!("Managed model archive is not a ZIP: {error}")))?;
    if archive.len() != MODEL_FILES.len() + 1 {
        return Err(model_error("Managed model archive layout is invalid"));
    }
    std::fs::create_dir_all(destination).map_err(AppCommandError::io)?;
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            model_error(format!("Managed model archive is unreadable: {error}"))
        })?;
        let name = safe_entry_name(&entry)?;
        if !seen.insert(name.to_string()) {
            return Err(model_error(
                "Managed model archive contains duplicate files",
            ));
        }
        extract_entry(&mut entry, &destination.join(&name), &name)?;
    }
    validate_directory(destination)
}

pub(super) fn validate_directory(directory: &Path) -> Result<ModelManifest, AppCommandError> {
    let manifest = read_manifest(directory)?;
    for expected in MODEL_FILES {
        verify_file(
            &directory.join(expected.name),
            expected.size,
            expected.sha256,
        )?;
    }
    Ok(manifest)
}

pub(super) fn validate_layout(directory: &Path) -> Result<(), AppCommandError> {
    read_manifest(directory)?;
    for expected in MODEL_FILES {
        reject_link_or_non_file(&directory.join(expected.name))?;
        if std::fs::metadata(directory.join(expected.name))
            .map_err(AppCommandError::io)?
            .len()
            != expected.size
        {
            return Err(model_error("Managed model file size mismatch"));
        }
    }
    Ok(())
}

pub(super) fn stage_legacy(source: &Path, destination: &Path) -> Result<(), AppCommandError> {
    std::fs::create_dir_all(destination).map_err(AppCommandError::io)?;
    for expected in MODEL_FILES {
        let source_file = source.join(expected.name);
        verify_file(&source_file, expected.size, expected.sha256)?;
        copy_file(&source_file, &destination.join(expected.name))?;
    }
    let manifest = serde_json::to_vec_pretty(&ModelManifest::legacy())
        .map_err(|error| model_error(format!("Serialize managed model manifest: {error}")))?;
    write_new(&destination.join(MANIFEST_FILE), &manifest)?;
    validate_directory(destination).map(|_| ())
}

fn safe_entry_name(entry: &zip::read::ZipFile<'_>) -> Result<String, AppCommandError> {
    let name = entry.name();
    let allowed = name == MANIFEST_FILE || MODEL_FILES.iter().any(|file| file.name == name);
    let unsafe_mode = entry
        .unix_mode()
        .is_some_and(|mode| (mode & 0o170000) == 0o120000);
    if !allowed
        || entry.is_dir()
        || unsafe_mode
        || Path::new(name).file_name() != Some(OsStr::new(name))
    {
        return Err(model_error(
            "Managed model archive contains an unsafe entry",
        ));
    }
    Ok(name.to_string())
}

fn extract_entry<R: Read>(
    source: &mut R,
    target: &Path,
    name: &str,
) -> Result<(), AppCommandError> {
    let limit = if name == MANIFEST_FILE {
        MAX_MANIFEST_BYTES
    } else {
        MODEL_FILES
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.size)
            .ok_or_else(|| model_error("Managed model archive file is unsupported"))?
    };
    let mut file = new_file(target)?;
    let written =
        std::io::copy(&mut source.take(limit + 1), &mut file).map_err(AppCommandError::io)?;
    if written > limit {
        let _ = std::fs::remove_file(target);
        return Err(model_error(
            "Managed model archive file exceeds its size limit",
        ));
    }
    file.sync_all().map_err(AppCommandError::io)
}

fn read_manifest(directory: &Path) -> Result<ModelManifest, AppCommandError> {
    let path = directory.join(MANIFEST_FILE);
    reject_link_or_non_file(&path)?;
    let manifest: ModelManifest = serde_json::from_slice(&read_limited(&path, MAX_MANIFEST_BYTES)?)
        .map_err(|error| model_error(format!("Managed model manifest is invalid: {error}")))?;
    manifest.validate()?;
    Ok(manifest)
}

fn verify_file(path: &Path, size: u64, checksum: &str) -> Result<(), AppCommandError> {
    reject_link_or_non_file(path)?;
    let metadata = std::fs::metadata(path).map_err(AppCommandError::io)?;
    if metadata.len() != size {
        return Err(model_error("Managed model file size mismatch"));
    }
    let mut file = File::open(path).map_err(AppCommandError::io)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(AppCommandError::io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if format!("{:x}", hasher.finalize()) != checksum {
        return Err(model_error("Managed model file checksum mismatch"));
    }
    Ok(())
}

fn reject_link_or_non_file(path: &Path) -> Result<(), AppCommandError> {
    let metadata = std::fs::symlink_metadata(path).map_err(AppCommandError::io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(model_error("Managed model resource is not a regular file"));
    }
    Ok(())
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, AppCommandError> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(AppCommandError::io)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(AppCommandError::io)?;
    if bytes.len() as u64 > limit {
        return Err(model_error("Managed model manifest exceeds its size limit"));
    }
    Ok(bytes)
}

fn copy_file(source: &Path, target: &Path) -> Result<(), AppCommandError> {
    let mut input = File::open(source).map_err(AppCommandError::io)?;
    let mut output = new_file(target)?;
    std::io::copy(&mut input, &mut output).map_err(AppCommandError::io)?;
    output.sync_all().map_err(AppCommandError::io)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), AppCommandError> {
    let mut file = new_file(path)?;
    file.write_all(bytes).map_err(AppCommandError::io)?;
    file.sync_all().map_err(AppCommandError::io)
}

fn new_file(path: &Path) -> Result<File, AppCommandError> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(AppCommandError::io)
}
