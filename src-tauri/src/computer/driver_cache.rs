use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use super::driver::{DriverArtifact, DRIVER_COMMAND, DRIVER_VERSION};
use crate::acp::error::AcpError;

const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 100 * 1024 * 1024;

pub fn cached_path() -> PathBuf {
    crate::paths::iyw_claw_user_dir()
        .join("runtime/tools/cua-driver")
        .join(DRIVER_VERSION)
        .join(crate::acp::registry::current_platform())
        .join(format!("{DRIVER_COMMAND}{}", std::env::consts::EXE_SUFFIX))
}

pub async fn ensure(
    artifact: &DriverArtifact,
    progress: impl Fn(&str),
) -> Result<PathBuf, AcpError> {
    let destination = cached_path();
    if destination.is_file() && verify(&destination, artifact.executable_sha256).is_ok() {
        return Ok(destination);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(failed)?;
    let mut last_error = String::new();
    for url in crate::github_mirror::download_candidates(artifact.url) {
        match fetch_archive(&client, &url, &progress).await {
            Ok(bytes) if digest(&bytes) == artifact.archive_sha256 => {
                return install_archive(artifact, &bytes, &destination);
            }
            Ok(_) => last_error = "cua-driver archive digest mismatch".into(),
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(failed(last_error))
}

async fn fetch_archive(
    client: &reqwest::Client,
    url: &str,
    progress: &impl Fn(&str),
) -> Result<Vec<u8>, AcpError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(failed)?
        .error_for_status()
        .map_err(failed)?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARCHIVE_BYTES as u64)
    {
        return Err(failed("cua-driver archive exceeds its size limit"));
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    let mut reported = 0;
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.map_err(failed)?);
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err(failed("cua-driver archive exceeds its size limit"));
        }
        if bytes.len().saturating_sub(reported) >= 1024 * 1024 {
            progress(&format!(
                "Downloading... {:.1} MB",
                bytes.len() as f64 / 1_048_576.0
            ));
            reported = bytes.len();
        }
    }
    Ok(bytes)
}

fn install_archive(
    artifact: &DriverArtifact,
    bytes: &[u8],
    destination: &Path,
) -> Result<PathBuf, AcpError> {
    let executable = extract_executable(artifact.url.ends_with(".zip"), bytes)?;
    if digest(&executable) != artifact.executable_sha256 {
        return Err(failed("cua-driver executable digest mismatch"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| failed("cua-driver cache directory is invalid"))?;
    std::fs::create_dir_all(parent).map_err(failed)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
    std::io::Write::write_all(&mut staged, &executable).map_err(failed)?;
    staged.as_file().sync_all().map_err(failed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .map_err(failed)?;
    }
    staged.persist(destination).map_err(failed)?;
    Ok(destination.to_owned())
}

fn extract_executable(zip: bool, bytes: &[u8]) -> Result<Vec<u8>, AcpError> {
    let name = format!("{DRIVER_COMMAND}{}", std::env::consts::EXE_SUFFIX);
    if zip {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(failed)?;
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(failed)?;
            if !file.is_file()
                || Path::new(file.name())
                    .file_name()
                    .is_none_or(|n| n != name.as_str())
            {
                continue;
            }
            return read_executable(file);
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(bytes)));
        for entry in archive.entries().map_err(failed)? {
            let file = entry.map_err(failed)?;
            if !file.header().entry_type().is_file()
                || file
                    .path()
                    .map_err(failed)?
                    .file_name()
                    .is_none_or(|n| n != name.as_str())
            {
                continue;
            }
            return read_executable(file);
        }
    }
    Err(failed(
        "cua-driver executable missing from the pinned archive",
    ))
}

fn read_executable(reader: impl Read) -> Result<Vec<u8>, AcpError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_EXECUTABLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(failed)?;
    if bytes.len() as u64 > MAX_EXECUTABLE_BYTES {
        return Err(failed("cua-driver executable exceeds its size limit"));
    }
    Ok(bytes)
}

pub fn verify(path: &Path, expected: &str) -> Result<(), AcpError> {
    let bytes = std::fs::read(path).map_err(failed)?;
    if digest(&bytes) != expected {
        return Err(failed("cua-driver executable digest mismatch"));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn failed(error: impl std::fmt::Display) -> AcpError {
    AcpError::DownloadFailed(error.to_string())
}
