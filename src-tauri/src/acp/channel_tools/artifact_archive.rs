use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use zip::write::SimpleFileOptions;

use crate::chat_channel::attachments::MIB;

const MAX_ARCHIVE_BYTES: u64 = 100 * MIB;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

pub(super) struct ArtifactArchive {
    pub path: PathBuf,
    _directory: TempDir,
}

pub(super) async fn prepare(path: String) -> Result<ArtifactArchive, &'static str> {
    tokio::task::spawn_blocking(move || archive_directory(Path::new(&path)))
        .await
        .map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?
}

fn archive_directory(source: &Path) -> Result<ArtifactArchive, &'static str> {
    let source = source.canonicalize().map_err(|_| "FILE_NOT_FOUND")?;
    if !source.is_dir() {
        return Err("FILE_NOT_READABLE");
    }
    let directory = tempfile::tempdir().map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?;
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    let path = directory.path().join(format!("{name}.zip"));
    let file = std::fs::File::create(&path).map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?;
    let mut archive = zip::ZipWriter::new(file);
    write_entries(&mut archive, &source)?;
    let file = archive.finish().map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?;
    if file.metadata().map_err(|_| "FILE_NOT_READABLE")?.len() > MAX_ARCHIVE_BYTES {
        return Err("DIRECTORY_TOO_LARGE");
    }
    Ok(ArtifactArchive {
        path,
        _directory: directory,
    })
}

fn write_entries(
    archive: &mut zip::ZipWriter<std::fs::File>,
    source: &Path,
) -> Result<(), &'static str> {
    let mut remaining = MAX_ARCHIVE_BYTES;
    for (index, entry) in walkdir::WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if index > MAX_ARCHIVE_ENTRIES {
            return Err("DIRECTORY_TOO_LARGE");
        }
        let entry = entry.map_err(|_| "FILE_NOT_READABLE")?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| "FILE_NOT_READABLE")?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let name = relative.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            archive
                .add_directory(format!("{name}/"), SimpleFileOptions::default())
                .map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?;
        } else {
            remaining = add_file(archive, &entry, (&name, remaining))?;
        }
    }
    Ok(())
}

fn add_file(
    archive: &mut zip::ZipWriter<std::fs::File>,
    entry: &walkdir::DirEntry,
    capacity: (&str, u64),
) -> Result<u64, &'static str> {
    if !entry.file_type().is_file() {
        return Err("DIRECTORY_UNSUPPORTED_ENTRY");
    }
    let (name, remaining) = capacity;
    let mut file = std::fs::File::open(entry.path()).map_err(|_| "FILE_NOT_READABLE")?;
    if file.metadata().map_err(|_| "FILE_NOT_READABLE")?.len() > remaining {
        return Err("DIRECTORY_TOO_LARGE");
    }
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?;
    let mut copied = 0;
    let mut buffer = [0u8; COPY_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(|_| "FILE_NOT_READABLE")?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        if copied > remaining {
            return Err("DIRECTORY_TOO_LARGE");
        }
        archive
            .write_all(&buffer[..read])
            .map_err(|_| "DIRECTORY_ARCHIVE_FAILED")?;
    }
    Ok(remaining - copied)
}
