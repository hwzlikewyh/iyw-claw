use std::path::Path;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;

use crate::web::event_bridge::EventEmitter;

use super::download_support::{source_host, stream_to_file, verify_archive};
use super::spec::ComponentSpec;
use super::{emit_event, RuntimeBootstrapEventKind};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) async fn download_archive(
    spec: &ComponentSpec,
    destination: &Path,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<(), String> {
    if destination.is_file() {
        match verify_archive(destination, spec).await {
            Ok(true) => {
                let bytes = tokio::fs::metadata(destination)
                    .await
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                tracing::info!(
                    task_id,
                    tool_id = spec.kind.tool_id(),
                    version = spec.version,
                    cache = "hit",
                    bytes,
                    "[runtime-bootstrap] fallback archive cache hit"
                );
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
            Ok(false) => {
                tracing::warn!(
                    task_id,
                    tool_id = spec.kind.tool_id(),
                    version = spec.version,
                    cache = "corrupt",
                    "[runtime-bootstrap] fallback archive cache rejected"
                );
            }
            Err(error) => {
                tracing::warn!(
                    task_id,
                    tool_id = spec.kind.tool_id(),
                    version = spec.version,
                    cache = "verify_failed",
                    error_detail_present = true,
                    "[runtime-bootstrap] fallback archive cache verification failed"
                );
                return Err(error);
            }
        }
        if let Err(error) = tokio::fs::remove_file(destination).await {
            tracing::error!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                cache = "remove_failed",
                "[runtime-bootstrap] failed to remove corrupt fallback cache"
            );
            return Err(format!("failed to remove corrupt cache: {error}"));
        }
    }
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| {
            tracing::error!(
                task_id,
                tool_id = spec.kind.tool_id(),
                phase = "http_client",
                "[runtime-bootstrap] fallback HTTP client unavailable"
            );
            format!("failed to build fallback HTTP client: {error}")
        })?;
    let mut last_error = String::new();
    // Every mirror is exhausted before the official upstream is tried; the
    // pinned SHA-256 below is what makes trusting a proxy safe here.
    for (label, url) in spec.sources() {
        let label = label.as_str();
        announce_source(spec, task_id, emitter, label);
        let source_started = Instant::now();
        let host = source_host(url);
        tracing::info!(
            task_id,
            tool_id = spec.kind.tool_id(),
            version = spec.version,
            source = label,
            source_host = %host,
            phase = "begin",
            "[runtime-bootstrap] fallback source download started"
        );
        match download_once(&client, url, destination, spec, task_id, emitter, label).await {
            Ok(bytes) => {
                tracing::info!(
                    task_id,
                    tool_id = spec.kind.tool_id(),
                    version = spec.version,
                    source = label,
                    source_host = %host,
                    outcome = "downloaded",
                    bytes,
                    duration_ms = source_started.elapsed().as_millis() as u64,
                    "[runtime-bootstrap] fallback source download finished"
                );
                return Ok(());
            }
            Err(error) => {
                last_error = format!("{label}: {error}");
                tracing::warn!(
                    task_id,
                    tool_id = spec.kind.tool_id(),
                    version = spec.version,
                    source = label,
                    source_host = %host,
                    outcome = "failed",
                    error_detail_present = true,
                    duration_ms = source_started.elapsed().as_millis() as u64,
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
    source: &str,
) -> Result<u64, String> {
    let host = source_host(url);
    let response = match client.get(url).timeout(DOWNLOAD_TIMEOUT).send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                source,
                source_host = %host,
                phase = "http_request",
                timeout = error.is_timeout(),
                connect = error.is_connect(),
                "[runtime-bootstrap] fallback HTTP request failed"
            );
            return Err(format!("request failed: {error}"));
        }
    };
    let status = response.status();
    let total = response.content_length();
    tracing::info!(
        task_id,
        tool_id = spec.kind.tool_id(),
        version = spec.version,
        source,
        source_host = %host,
        http_status = %status,
        content_length = ?total,
        "[runtime-bootstrap] fallback HTTP response received"
    );
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let partial = destination.with_extension("part");
    let mut file = tokio::fs::File::create(&partial).await.map_err(|error| {
        tracing::warn!(
            task_id,
            tool_id = spec.kind.tool_id(),
            source,
            phase = "create_partial",
            "[runtime-bootstrap] fallback partial file creation failed"
        );
        format!("failed to create {}: {error}", partial.display())
    })?;
    let downloaded = match stream_to_file(response, &mut file, total, spec, task_id, emitter).await
    {
        Ok(downloaded) => downloaded,
        Err(error) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                source,
                phase = "stream",
                "[runtime-bootstrap] fallback download stream failed"
            );
            return Err(error);
        }
    };
    if let Err(error) = file.flush().await {
        tracing::warn!(
            task_id,
            tool_id = spec.kind.tool_id(),
            source,
            phase = "flush",
            downloaded_bytes = downloaded,
            "[runtime-bootstrap] fallback download flush failed"
        );
        return Err(format!("failed to flush download: {error}"));
    }
    drop(file);
    match verify_archive(&partial, spec).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                source,
                phase = "integrity",
                downloaded_bytes = downloaded,
                "[runtime-bootstrap] fallback archive integrity check failed"
            );
            let _ = tokio::fs::remove_file(&partial).await;
            return Err("downloaded archive SHA-256 does not match the pinned digest".to_string());
        }
        Err(error) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                source,
                phase = "integrity",
                error_detail_present = true,
                "[runtime-bootstrap] fallback archive integrity check unavailable"
            );
            return Err(error);
        }
    }
    if let Err(error) = tokio::fs::rename(&partial, destination).await {
        tracing::warn!(
            task_id,
            tool_id = spec.kind.tool_id(),
            source,
            phase = "finalize",
            downloaded_bytes = downloaded,
            "[runtime-bootstrap] fallback archive finalization failed"
        );
        return Err(format!("failed to finalize download: {error}"));
    }
    Ok(downloaded)
}
