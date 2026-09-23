use std::path::Path;

use crate::app_error::AppCommandError;

use super::restore::ConflictPolicy;

pub(super) fn restore_file(
    source: &Path,
    destination: (&Path, &Path),
    policy: ConflictPolicy,
) -> Result<bool, AppCommandError> {
    let (base, target) = destination;
    if policy == ConflictPolicy::SkipExisting && std::fs::symlink_metadata(target).is_ok() {
        return Ok(false);
    }
    let parent = target.parent().ok_or_else(super::unknown_format_error)?;
    require_safe_parent(base, parent)?;
    require_closed_database(target)?;
    std::fs::create_dir_all(parent).map_err(AppCommandError::io)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(AppCommandError::io)?;
    let mut input = std::fs::File::open(source).map_err(AppCommandError::io)?;
    std::io::copy(&mut input, temporary.as_file_mut()).map_err(super::map_disk_full)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(AppCommandError::io)?;
    let published = match policy {
        ConflictPolicy::Overwrite => temporary.persist(target),
        ConflictPolicy::SkipExisting => temporary.persist_noclobber(target),
    };
    match published {
        Ok(_) => Ok(true),
        Err(error)
            if policy == ConflictPolicy::SkipExisting
                && error.error.kind() == std::io::ErrorKind::AlreadyExists =>
        {
            Ok(false)
        }
        Err(error) => Err(AppCommandError::io(error.error)),
    }
}

pub(super) fn require_closed_database(path: &Path) -> Result<(), AppCommandError> {
    if path.extension().is_none_or(|extension| extension != "db") {
        return Ok(());
    }
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        if Path::new(&sidecar).exists() {
            return Err(AppCommandError::invalid_input(
                "Close all agents using the destination session database before restoring",
            ));
        }
    }
    Ok(())
}

fn require_safe_parent(base: &Path, parent: &Path) -> Result<(), AppCommandError> {
    let relative = parent
        .strip_prefix(base)
        .map_err(|_| super::unknown_format_error())?;
    let mut current = base.to_path_buf();
    reject_symlink(&current)?;
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(super::unknown_format_error());
        }
        current.push(component);
        reject_symlink(&current)?;
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), AppCommandError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppCommandError::invalid_input(
            "Restore destination contains a symbolic link",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}
