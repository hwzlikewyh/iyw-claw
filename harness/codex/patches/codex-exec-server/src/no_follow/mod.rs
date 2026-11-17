use crate::FileMetadata;
use std::io;
use std::path::Path;
#[cfg(windows)]
use std::time::SystemTime;
#[cfg(windows)]
use std::time::UNIX_EPOCH;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

fn invalid_output(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}

/// A destination bound before sandboxed tools run. Only ordinary path-based files are truncated.
pub struct BoundOutput {
    file: std::fs::File,
    destination: Destination,
}

enum Destination {
    Path(std::path::PathBuf),
    Stream { is_stdout: bool },
}

impl BoundOutput {
    pub fn open(path: &Path) -> io::Result<Self> {
        // Check raw syntax before absolute() or components() discards directory markers.
        let bytes = path.as_os_str().as_encoded_bytes();
        let leaf = bytes
            .rsplit(|b| *b == b'/' || (cfg!(windows) && *b == b'\\'))
            .next();
        if matches!(leaf, None | Some(b"" | b"." | b"..")) {
            return Err(invalid_output("output must name a file"));
        }
        #[cfg(windows)]
        if bytes.eq_ignore_ascii_case(b"NUL") || bytes.eq_ignore_ascii_case(br"\\.\NUL") {
            return Ok(Self {
                file: windows::open_null_output()?,
                destination: Destination::Stream { is_stdout: false },
            });
        }
        #[cfg(unix)]
        if let Some(output) = unix::inherited_output(path)? {
            return Ok(output);
        }
        #[cfg(unix)]
        let path = unix::output_path(path)?;
        #[cfg(windows)]
        let path = std::path::absolute(path)?;
        let file = imp::open_output(&path)?;
        if file.metadata()?.is_file() {
            imp::reject_multiple_links(&file)?;
        }
        Ok(Self {
            file,
            destination: Destination::Path(path),
        })
    }

    /// Whether the caller explicitly selected the inherited standard output stream.
    pub fn is_stdout(&self) -> bool {
        matches!(self.destination, Destination::Stream { is_stdout: true })
    }

    pub fn write(mut self, contents: &[u8]) -> io::Result<()> {
        use std::io::Write;
        if let Destination::Path(path) = &self.destination {
            imp::verify_output_path(&self.file, path)?;
            if self.file.metadata()?.is_file() {
                imp::reject_multiple_links(&self.file)?;
                self.file.set_len(0)?;
            }
        }
        self.file.write_all(contents)?;
        // A caller may reopen the pathname after a successful exit. Detect a visible swap.
        if let Destination::Path(path) = &self.destination {
            imp::verify_output_path(&self.file, path)?;
        }
        Ok(())
    }
}

pub(crate) fn open_file(path: &Path) -> io::Result<std::fs::File> {
    imp::open_file_sync(path)
}

pub(crate) async fn write_file(path: &Path, contents: Vec<u8>) -> io::Result<()> {
    imp::write_file(path.to_path_buf(), contents).await
}

#[cfg(unix)]
pub(crate) async fn metadata(path: &Path) -> io::Result<FileMetadata> {
    imp::metadata(path.to_path_buf()).await
}

#[cfg(windows)]
pub(crate) async fn metadata(path: &Path) -> io::Result<FileMetadata> {
    imp::metadata(path.to_path_buf())
        .await
        .map(|metadata| FileMetadata {
            is_directory: metadata.is_dir(),
            is_file: metadata.is_file(),
            is_symlink: false,
            size: metadata.len(),
            created_at_ms: metadata.created().ok().map_or(0, system_time_to_unix_ms),
            modified_at_ms: metadata.modified().ok().map_or(0, system_time_to_unix_ms),
        })
}

#[cfg(windows)]
fn system_time_to_unix_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

pub(crate) async fn create_directory(path: &Path, recursive: bool) -> io::Result<()> {
    imp::create_directory(path.to_path_buf(), recursive).await
}

pub(crate) async fn remove(path: &Path, recursive: bool, force: bool) -> io::Result<()> {
    imp::remove(path.to_path_buf(), recursive, force).await
}
