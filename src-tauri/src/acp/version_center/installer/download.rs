use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{redirect::Policy, Url};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::acp::version_center::types::ToolOffer;
use crate::app_error::AppCommandError;

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

static DOWNLOAD_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(15 * 60))
        .redirect(Policy::none())
        .build()
        .map_err(|error| error.to_string())
});

pub fn validate_ticket(
    offer: &ToolOffer,
    url: &str,
    size: i64,
    sha256: &str,
    signature: &str,
) -> Result<(), AppCommandError> {
    let parsed = Url::parse(url)
        .map_err(|_| AppCommandError::invalid_input("Managed tool download URL is invalid"))?;
    let allow_http = cfg!(debug_assertions) && parsed.host_str() == Some("127.0.0.1");
    if !(parsed.scheme() == "https" || allow_http)
        || parsed.username() != ""
        || parsed.password().is_some()
        || size <= 0
        || size as u64 > MAX_ARCHIVE_BYTES
        || size != offer.artifact.size
        || !sha256.eq_ignore_ascii_case(&offer.artifact.sha256)
        || signature.trim().is_empty()
    {
        return Err(AppCommandError::invalid_input(
            "Managed tool download ticket was rejected",
        ));
    }
    Ok(())
}

/// Download `url` to `path`, verifying size and SHA-256.
///
/// `on_progress` is called with `(bytes_downloaded, total_bytes)` after each
/// chunk so callers can emit progress events without blocking the download.
/// Pass `None` to skip progress reporting.
pub async fn download_archive(
    url: &str,
    path: &Path,
    expected_size: i64,
    expected_sha256: &str,
    on_progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), AppCommandError> {
    let client = DOWNLOAD_CLIENT.as_ref().map_err(|error| {
        AppCommandError::configuration_invalid("Managed tool download client is unavailable")
            .with_detail(error.clone())
    })?;
    let response = client.get(url).send().await.map_err(|error| {
        AppCommandError::network("Managed tool download failed").with_detail(error.to_string())
    })?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|size| size != expected_size as u64)
    {
        return Err(AppCommandError::network(
            "Managed tool download was rejected",
        ));
    }
    let mut output = tokio::fs::File::create(path)
        .await
        .map_err(AppCommandError::io)?;
    let mut stream = response.bytes_stream();
    let mut total = 0_u64;
    let mut hasher = Sha256::new();
    // Throttle progress callbacks: emit at most once per 128 KiB to avoid
    // flooding the event bus on fast connections.
    let mut last_reported = 0_u64;
    const PROGRESS_GRANULARITY: u64 = 128 * 1024;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            AppCommandError::network("Managed tool download was interrupted")
                .with_detail(error.to_string())
        })?;
        total = total.saturating_add(chunk.len() as u64);
        if total > expected_size as u64 || total > MAX_ARCHIVE_BYTES {
            return Err(AppCommandError::invalid_input(
                "Managed tool archive is too large",
            ));
        }
        hasher.update(&chunk);
        output
            .write_all(&chunk)
            .await
            .map_err(AppCommandError::io)?;
        if let Some(cb) = on_progress {
            if total - last_reported >= PROGRESS_GRANULARITY {
                cb(total, expected_size as u64);
                last_reported = total;
            }
        }
    }
    // Final progress tick at 100 %
    if let Some(cb) = on_progress {
        cb(total, expected_size as u64);
    }
    output.flush().await.map_err(AppCommandError::io)?;
    let actual = format!("{:x}", hasher.finalize());
    if total != expected_size as u64 || !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(AppCommandError::invalid_input(
            "Managed tool archive integrity check failed",
        ));
    }
    Ok(())
}
