use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::skill_package::ValidatedSkillPackage;
use crate::app_error::AppCommandError;
use serde::Serialize;
use std::fs;
use std::path::Path;

pub(super) fn active_paths() -> Result<AgentStoragePaths, AppCommandError> {
    AgentStoragePaths::active().ok_or_else(|| {
        AppCommandError::agent_storage_not_initialized("Agent storage is not initialized")
    })
}

pub(super) fn write_package(
    root: &Path,
    package: &ValidatedSkillPackage,
) -> Result<(), AppCommandError> {
    for file in &package.files {
        let target = root.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(storage_error)?;
        }
        fs::write(target, &file.bytes).map_err(storage_error)?;
    }
    Ok(())
}

pub(super) fn write_json_atomically(
    target: &Path,
    value: &impl Serialize,
) -> Result<(), AppCommandError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        AppCommandError::configuration_invalid("Failed to serialize plugin activation pointer")
            .with_detail(error.to_string())
    })?;
    write_bytes_atomically(target, &bytes)
}

pub(super) fn write_bytes_atomically(target: &Path, bytes: &[u8]) -> Result<(), AppCommandError> {
    use std::io::Write;
    let parent = target
        .parent()
        .ok_or_else(|| storage_error("Plugin storage parent is missing"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(storage_error)?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(storage_error)?;
    temporary
        .persist(target)
        .map_err(|error| storage_error(error.error))?;
    Ok(())
}

pub(super) fn restore_backup(backup: &Path, target: &Path) -> Result<(), AppCommandError> {
    if backup.exists() && !target.exists() {
        fs::rename(backup, target).map_err(storage_error)?;
    } else if backup.exists() {
        return Err(storage_error(
            "Plugin version directory exists; rollback backup retained",
        ));
    }
    Ok(())
}

pub(super) fn restore_current(
    plugin_root: &Path,
    previous: Option<&[u8]>,
) -> Result<(), AppCommandError> {
    let current = plugin_root.join("current.json");
    match previous {
        Some(bytes) => {
            write_bytes_atomically(&current, bytes)?;
        }
        None => match fs::remove_file(current) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(storage_error(error)),
        },
    }
    Ok(())
}

pub(super) fn storage_error(error: impl ToString) -> AppCommandError {
    AppCommandError::io_error("Failed to update managed plugin storage")
        .with_detail(error.to_string())
}

pub(super) fn read_pointer_bytes(path: &Path) -> Result<Option<Vec<u8>>, AppCommandError> {
    use std::io::Read;
    const MAX_POINTER_BYTES: u64 = 128 * 1024;
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(storage_error(error)),
    };
    if !metadata.is_file() || is_link(&metadata) || metadata.len() > MAX_POINTER_BYTES {
        return Err(storage_error("Invalid installed plugin pointer"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_POINTER_BYTES + 1).read_to_end(&mut bytes))
        .map_err(storage_error)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(storage_error(
            "Installed plugin pointer changed while reading",
        ));
    }
    Ok(Some(bytes))
}

pub(super) fn require_plugin_path(root: &Path) -> Result<(), AppCommandError> {
    let base = active_paths()?.plugins_dir();
    let relative = root.strip_prefix(&base).map_err(storage_error)?;
    if relative.components().count() != 1
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(storage_error("Plugin path escapes managed storage"));
    }
    for path in [
        root.to_path_buf(),
        root.join("versions"),
        root.join("receipts"),
    ] {
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_dir() || is_link(&metadata) => {
                return Err(storage_error("Managed plugin path is not a real directory"))
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(storage_error(error)),
        }
    }
    Ok(())
}

pub(super) fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}
