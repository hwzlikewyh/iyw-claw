use std::io::Write;
use std::path::{Path, PathBuf};

use crate::app_error::AppCommandError;

pub(super) fn snapshot_for_backup(
    root: &Path,
    snapshot_root: &Path,
    _lock: &std::fs::File,
) -> Result<Vec<(String, PathBuf)>, AppCommandError> {
    std::fs::create_dir_all(snapshot_root).map_err(AppCommandError::io)?;
    let mut documents = Vec::new();
    for file_name in super::USER_MEMORY_BACKUP_FILES {
        let source = root.join(file_name);
        let snapshot = snapshot_root.join(file_name);
        if copy_regular_file_no_follow(&source, &snapshot)? {
            documents.push((file_name.to_string(), snapshot));
        }
    }
    Ok(documents)
}

fn copy_regular_file_no_follow(source: &Path, destination: &Path) -> Result<bool, AppCommandError> {
    reject_symlink_components(source)?;
    match std::fs::symlink_metadata(source) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(AppCommandError::io(error)),
        Ok(metadata) if !metadata.file_type().is_file() => return Ok(false),
        Ok(_) => {}
    }
    let mut input = open_read_no_follow(source).map_err(AppCommandError::io)?;
    if !input.metadata().map_err(AppCommandError::io)?.is_file() {
        return Ok(false);
    }
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(AppCommandError::io)?;
    std::io::copy(&mut input, &mut output).map_err(AppCommandError::io)?;
    output.flush().map_err(AppCommandError::io)?;
    output.sync_all().map_err(AppCommandError::io)?;
    Ok(true)
}

fn reject_symlink_components(path: &Path) -> Result<(), AppCommandError> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppCommandError::permission_denied(
                    "User memory backup paths cannot contain symlinks",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppCommandError::io(error)),
        }
        current = candidate.parent();
    }
    Ok(())
}

#[cfg(unix)]
fn open_read_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_read_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_read_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}
