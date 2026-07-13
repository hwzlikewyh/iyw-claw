use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::profile_import::ProfileImportError;

pub(super) fn create_dir(path: &Path) -> Result<(), ProfileImportError> {
    fs::create_dir_all(path).map_err(|error| io_error(path, error))
}

pub(super) fn read_dir(path: &Path) -> Result<fs::ReadDir, ProfileImportError> {
    fs::read_dir(path).map_err(|error| io_error(path, error))
}

pub(super) fn canonicalize(path: &Path) -> Result<PathBuf, ProfileImportError> {
    fs::canonicalize(path).map_err(|error| io_error(path, error))
}

pub(super) fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

pub(super) fn io_error(path: &Path, source: std::io::Error) -> ProfileImportError {
    ProfileImportError::Io {
        path: path.to_path_buf(),
        source,
    }
}
