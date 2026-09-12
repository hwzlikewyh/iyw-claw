use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::acp::delegation::image_format::detect_mime;
use crate::acp::delegation::image_loader::MAX_IMAGE_BYTES;
use sha2::{Digest, Sha256};

const IMAGE_HEADER_BYTES: u64 = 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

pub(super) async fn materialize_url(
    directory: &Path,
    source: &str,
) -> Result<Option<PathBuf>, &'static str> {
    let downloaded = crate::remote_image::network::download(source, MAX_IMAGE_BYTES)
        .await
        .map_err(|error| {
            tracing::warn!(
                error_code = ?error.code,
                error_message = %error.message,
                error_detail = ?error.detail,
                "[task-artifacts] URL download failed"
            );
            "image_download_failed"
        })?;
    let Some(mime) = detect_mime(&downloaded.bytes) else {
        return Ok(None);
    };
    tracing::info!(
        bytes = downloaded.bytes.len(),
        mime_type = mime,
        redirects = downloaded.redirects,
        "[task-artifacts] image URL downloaded for delivery"
    );
    publish_image(directory, downloaded.bytes, mime)
        .await
        .map(Some)
}

pub(super) async fn materialize_local(
    directory: &Path,
    source: &Path,
) -> Result<Option<PathBuf>, &'static str> {
    let directory = directory.to_owned();
    let source = source.to_owned();
    tokio::task::spawn_blocking(move || copy_local_image(&directory, &source))
        .await
        .map_err(storage_error)?
        .map_err(storage_error)
}

fn copy_local_image(directory: &Path, source: &Path) -> io::Result<Option<PathBuf>> {
    if !std::fs::metadata(source)?.is_file() {
        return Ok(None);
    }
    let mut file = std::fs::File::open(source)?;
    let mut header = Vec::new();
    Read::by_ref(&mut file)
        .take(IMAGE_HEADER_BYTES)
        .read_to_end(&mut header)?;
    let Some(mime) = detect_mime(&header) else {
        return Ok(None);
    };
    file.seek(SeekFrom::Start(0))?;
    write_image(directory, &mut file, mime).map(Some)
}

async fn publish_image(
    directory: &Path,
    bytes: Vec<u8>,
    mime: &str,
) -> Result<PathBuf, &'static str> {
    let directory = directory.to_owned();
    let mime = mime.to_owned();
    tokio::task::spawn_blocking(move || write_image(&directory, &mut bytes.as_slice(), &mime))
        .await
        .map_err(storage_error)?
        .map_err(storage_error)
}

fn write_image(directory: &Path, source: &mut impl Read, mime: &str) -> io::Result<PathBuf> {
    let extension = match mime {
        "image/jpeg" => "jpg",
        "image/svg+xml" => "svg",
        value => value
            .strip_prefix("image/")
            .ok_or_else(|| io::Error::other("invalid image type"))?,
    };
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    let digest = copy_and_hash(source, &mut temporary)?;
    temporary.as_file().sync_all()?;
    let target = directory.join(format!("image-{digest}.{extension}"));
    let reused = match temporary.persist_noclobber(&target) {
        Ok(_) => false,
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&target)?;
            if !metadata.is_file()
                || copy_and_hash(&mut std::fs::File::open(&target)?, &mut io::sink())? != digest
            {
                return Err(io::Error::other(
                    "managed image content does not match its digest",
                ));
            }
            true
        }
        Err(error) => return Err(error.error),
    };
    tracing::info!(
        content_digest = digest,
        reused,
        "[task-artifacts] image materialized for delivery"
    );
    Ok(target)
}

fn copy_and_hash(source: &mut impl Read, target: &mut impl Write) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; COPY_BUFFER_BYTES];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        target.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
}

fn storage_error(error: impl std::fmt::Display) -> &'static str {
    tracing::warn!(error = %error, "[task-artifacts] image storage failed");
    "materialize_failed"
}
