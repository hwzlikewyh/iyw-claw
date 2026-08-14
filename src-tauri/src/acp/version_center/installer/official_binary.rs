use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::binary_cache;
use crate::acp::error::AcpError;
use crate::acp::registry::{self, AgentDistribution};
use crate::models::agent::AgentType;

use super::agent_archive::extract_archive;
use super::resumable::MAX_ARCHIVE_BYTES;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

struct OfficialBinaryInstall<'a> {
    paths: &'a AgentStoragePaths,
    agent_type: AgentType,
    version: &'a str,
    cmd: &'a str,
    url: &'a str,
    file_name: &'a str,
    archive: &'a Path,
    stage: &'a Path,
    on_progress: &'a dyn Fn(&str),
}

pub(super) async fn install_official_binary(
    paths: &AgentStoragePaths,
    agent_type: AgentType,
    version: &str,
    on_progress: &impl Fn(&str),
) -> Result<(), AcpError> {
    let (cmd, url, file_name) = official_spec(agent_type, version)?;
    let operation = uuid::Uuid::new_v4();
    let stage = paths
        .staging_dir()
        .join(format!("official-agent-{operation}"));
    let archive = paths
        .downloads_dir()
        .join(format!("official-agent-{operation}.archive"));
    tokio::fs::create_dir_all(paths.staging_dir())
        .await
        .map_err(download_error)?;
    tokio::fs::create_dir_all(paths.downloads_dir())
        .await
        .map_err(download_error)?;
    tokio::fs::create_dir(&stage)
        .await
        .map_err(download_error)?;
    let result = install_archive(OfficialBinaryInstall {
        paths,
        agent_type,
        version,
        cmd,
        url,
        file_name: &file_name,
        archive: &archive,
        stage: &stage,
        on_progress,
    })
    .await;
    let _ = tokio::fs::remove_file(&archive).await;
    let _ = tokio::fs::remove_dir_all(&stage).await;
    result
}

fn official_spec(
    agent_type: AgentType,
    version: &str,
) -> Result<(&'static str, &'static str, String), AcpError> {
    let AgentDistribution::Binary {
        version: built_in,
        cmd,
        platforms,
        ..
    } = registry::get_agent_meta(agent_type).distribution
    else {
        return Err(AcpError::protocol("Agent is not binary-based"));
    };
    if built_in != version {
        return Err(AcpError::DownloadFailed(
            "official fallback is only available for the built-in Agent version".into(),
        ));
    }
    let url = platforms
        .iter()
        .find(|item| item.platform == registry::current_platform())
        .map(|item| item.url)
        .ok_or_else(|| AcpError::DownloadFailed("official Agent binary is unavailable".into()))?;
    let parsed = reqwest::Url::parse(url).map_err(download_error)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(AcpError::DownloadFailed(
            "official Agent URL is not HTTPS".into(),
        ));
    }
    let file_name = parsed
        .path_segments()
        .and_then(Iterator::last)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AcpError::DownloadFailed("official Agent URL has no file name".into()))?;
    Ok((cmd, url, file_name.to_string()))
}

async fn install_archive(request: OfficialBinaryInstall<'_>) -> Result<(), AcpError> {
    (request.on_progress)("Fusion unavailable; using the pinned official Agent release");
    download_archive(request.url, request.archive).await?;
    let bytes = tokio::fs::read(request.archive)
        .await
        .map_err(download_error)?;
    let extracted = request.stage.join("extracted");
    extract_archive(&bytes, request.file_name, &extracted).map_err(app_error)?;
    let executable = executable_name(request.cmd);
    let source = binary_cache::find_binary_recursive(&extracted, &executable)
        .ok_or_else(|| AcpError::DownloadFailed("Agent executable is missing".into()))?;
    std::fs::copy(source, request.stage.join(&executable)).map_err(download_error)?;
    std::fs::remove_dir_all(extracted).map_err(download_error)?;
    binary_cache::activate_staged_binary(
        request.paths,
        registry::registry_id_for(request.agent_type),
        request.version,
        &executable,
        request.stage,
    )?;
    Ok(())
}

async fn download_archive(url: &str, destination: &Path) -> Result<(), AcpError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.stop();
            }
            if attempt.url().scheme() == "https" {
                attempt.follow()
            } else {
                attempt.error("official Agent redirect must remain HTTPS")
            }
        }))
        .build()
        .map_err(download_error)?;
    let response = client.get(url).send().await.map_err(download_error)?;
    if !response.status().is_success() {
        return Err(AcpError::DownloadFailed(format!(
            "official Agent download returned HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARCHIVE_BYTES)
    {
        return Err(AcpError::DownloadFailed(
            "official Agent archive is too large".into(),
        ));
    }
    stream_archive(response, destination).await
}

async fn stream_archive(response: reqwest::Response, destination: &Path) -> Result<(), AcpError> {
    let expected = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(destination)
        .await
        .map_err(download_error)?;
    let mut downloaded = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(download_error)?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_ARCHIVE_BYTES {
            return Err(AcpError::DownloadFailed(
                "official Agent archive is too large".into(),
            ));
        }
        file.write_all(&chunk).await.map_err(download_error)?;
    }
    file.flush().await.map_err(download_error)?;
    if expected.is_some_and(|size| size != downloaded) {
        return Err(AcpError::DownloadFailed(
            "official Agent archive length mismatch".into(),
        ));
    }
    Ok(())
}

fn executable_name(cmd: &str) -> String {
    if cfg!(windows) {
        format!("{cmd}.exe")
    } else {
        cmd.to_string()
    }
}

fn app_error(error: crate::app_error::AppCommandError) -> AcpError {
    let detail = error
        .detail
        .map(|detail| format!("{}: {detail}", error.message))
        .unwrap_or(error.message);
    AcpError::DownloadFailed(detail)
}

fn download_error(error: impl std::fmt::Display) -> AcpError {
    AcpError::DownloadFailed(error.to_string())
}
