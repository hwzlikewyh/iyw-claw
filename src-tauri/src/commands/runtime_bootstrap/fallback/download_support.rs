use std::path::Path;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::web::event_bridge::EventEmitter;

use super::spec::ComponentSpec;
use super::{emit_event, RuntimeBootstrapEventKind};

pub(super) fn source_host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) async fn verify_archive(path: &Path, spec: &ComponentSpec) -> Result<bool, String> {
    let Some(expected) = spec.expected_sha256 else {
        return Ok(true);
    };
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| format!("failed to verify {}: {error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected))
}

pub(super) async fn stream_to_file(
    response: reqwest::Response,
    file: &mut tokio::fs::File,
    total: Option<u64>,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<u64, String> {
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    let mut last_percent = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("download interrupted: {error}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("failed to write download: {error}"))?;
        downloaded += chunk.len() as u64;
        last_percent = emit_progress(total, downloaded, last_percent, spec, task_id, emitter);
    }
    Ok(downloaded)
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
