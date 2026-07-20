use std::fs::{File, OpenOptions};
use std::path::Path;

use crate::app_error::AppCommandError;

#[cfg(unix)]
pub(super) fn open_read_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
pub(super) fn open_read_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn open_read_no_follow(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(unix)]
pub(super) fn open_lock_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
pub(super) fn open_lock_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .create(true)
        .write(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn open_lock_no_follow(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).write(true).open(path)
}

#[cfg(unix)]
pub(super) fn replace_file(temp: &Path, target: &Path) -> Result<(), AppCommandError> {
    std::fs::rename(temp, target).map_err(AppCommandError::io)
}

#[cfg(windows)]
pub(super) fn replace_file(temp: &Path, target: &Path) -> Result<(), AppCommandError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let source = wide(temp);
    let destination = wide(target);
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(AppCommandError::io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(super) fn replace_file(temp: &Path, target: &Path) -> Result<(), AppCommandError> {
    std::fs::rename(temp, target).map_err(AppCommandError::io)
}

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> Result<(), AppCommandError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(AppCommandError::io)
}

#[cfg(not(unix))]
pub(super) fn sync_directory(_path: &Path) -> Result<(), AppCommandError> {
    Ok(())
}
