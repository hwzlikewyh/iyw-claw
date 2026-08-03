use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::web::event_bridge::EventEmitter;

use super::spec::ComponentSpec;
use super::{emit_event, RuntimeBootstrapEventKind};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) async fn download_verified(
    spec: &ComponentSpec,
    destination: &Path,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), String> {
    if archive_matches(destination, spec.sha256).await {
        emit_event(
            emitter,
            task_id,
            RuntimeBootstrapEventKind::Log,
            spec,
            None,
            format!("using cached {}", spec.asset),
        );
        return Ok(());
    }
    remove_invalid_cache(destination).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| format!("failed to build fallback HTTP client: {error}"))?;
    let mut last_error = String::new();
    for (label, url) in [
        ("mirror", &spec.mirror_url),
        ("official", &spec.official_url),
    ] {
        announce_source(spec, task_id, emitter, label);
        match download_once(&client, url, destination, spec, task_id, emitter).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = format!("{label}: {error}");
                tracing::warn!(
                    tool_id = spec.kind.tool_id(),
                    version = spec.version,
                    source = label,
                    error,
                    "[runtime-bootstrap] fallback source failed"
                );
            }
        }
    }
    Err(format!(
        "all fallback sources failed for {} ({last_error})",
        spec.asset
    ))
}

fn announce_source(spec: &ComponentSpec, task_id: &str, emitter: &EventEmitter, label: &str) {
    emit_event(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Log,
        spec,
        None,
        format!("downloading {} from {label} source", spec.asset),
    );
}

async fn download_once(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), String> {
    let response = client
        .get(url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let total = response.content_length();
    let partial = destination.with_extension("part");
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|error| format!("failed to create {}: {error}", partial.display()))?;
    let actual = stream_to_file(response, &mut file, total, spec, task_id, emitter).await?;
    file.flush()
        .await
        .map_err(|error| format!("failed to flush download: {error}"))?;
    drop(file);
    if !actual.eq_ignore_ascii_case(spec.sha256) {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err("SHA-256 checksum mismatch".to_string());
    }
    tokio::fs::rename(&partial, destination)
        .await
        .map_err(|error| format!("failed to finalize download: {error}"))
}

async fn stream_to_file(
    response: reqwest::Response,
    file: &mut tokio::fs::File,
    total: Option<u64>,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<String, String> {
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut last_percent = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("download interrupted: {error}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("failed to write download: {error}"))?;
        downloaded += chunk.len() as u64;
        last_percent = emit_progress(total, downloaded, last_percent, spec, task_id, emitter);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn emit_progress(
    total: Option<u64>,
    downloaded: u64,
    last_percent: Option<u8>,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Option<u8> {
    let total = total.filter(|value| *value > 0)?;
    let percent = ((downloaded.min(total) * 100) / total) as u8;
    if last_percent != Some(percent) {
        emit_event(
            emitter,
            task_id,
            RuntimeBootstrapEventKind::Progress,
            spec,
            Some(percent),
            String::new(),
        );
    }
    Some(percent)
}

async fn remove_invalid_cache(path: &Path) -> Result<(), String> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove invalid cache: {error}")),
    }
}

async fn archive_matches(path: &Path, expected_sha256: &str) -> bool {
    let path = path.to_path_buf();
    let expected = expected_sha256.to_string();
    tokio::task::spawn_blocking(move || match std::fs::File::open(path) {
        Ok(mut file) => {
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher).is_ok()
                && format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected)
        }
        Err(_) => false,
    })
    .await
    .unwrap_or(false)
}
