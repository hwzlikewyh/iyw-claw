use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

use crate::failure::{self, Failure};
use crate::model::EnvironmentArtifact;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

pub fn ensure_cached(
    client: &Client,
    cache_root: &Path,
    artifact: &EnvironmentArtifact,
    component: &str,
) -> Result<PathBuf> {
    let cache_dir = cache_root.join(&artifact.sha256);
    let destination = cache_dir.join("artifact");
    if valid_file(&destination, artifact.size_bytes, &artifact.sha256)? {
        emit(
            component,
            "cached",
            artifact.size_bytes,
            artifact.size_bytes,
        );
        return Ok(destination);
    }
    if destination.exists() {
        crate::retry::file("remove invalid artifact cache", &destination, || {
            fs::remove_file(&destination)
        })?;
    }
    if artifact.url.is_empty() {
        bail!("Fusion did not return a download URL for {component}")
    }
    crate::retry::file("create artifact cache", &cache_dir, || {
        fs::create_dir_all(&cache_dir)
    })?;
    let partial = cache_dir.join("artifact.part");
    crate::retry::run(component, || {
        download(client, &artifact.url, &partial, artifact, component)
    })?;
    crate::retry::file("activate verified artifact cache", &destination, || {
        fs::rename(&partial, &destination)
    })?;
    Ok(destination)
}

fn download(
    client: &Client,
    url: &str,
    destination: &Path,
    artifact: &EnvironmentArtifact,
    component: &str,
) -> Result<()> {
    let mut response = client.get(url).send().map_err(failure::network)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Err(Failure {
            code: "NETWORK",
            message: format!("{component} 下载失败（HTTP {status}）"),
            retryable: matches!(status, 408 | 429 | 500..=599),
        }
        .into());
    }
    let mut file = crate::retry::file("create partial artifact", destination, || {
        File::create(destination)
    })?;
    crate::paths::require_space(
        destination.parent().context("artifact has no parent")?,
        artifact.size_bytes,
    )?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    let mut last_progress = Instant::now();
    emit(component, "downloading", 0, artifact.size_bytes);
    loop {
        let read = response.read(&mut buffer).map_err(|error| {
            Failure::network(format!("{component} 下载连接中断（{:?}）", error.kind()))
        })?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .with_context(|| format!("write partial artifact: {}", destination.display()))?;
        hasher.update(&buffer[..read]);
        downloaded = downloaded.saturating_add(read as u64);
        if last_progress.elapsed() >= PROGRESS_INTERVAL {
            emit(component, "downloading", downloaded, artifact.size_bytes);
            last_progress = Instant::now();
        }
        if downloaded > artifact.size_bytes {
            bail!("downloaded artifact exceeds the declared size")
        }
    }
    file.sync_all()
        .with_context(|| format!("flush partial artifact: {}", destination.display()))?;
    let actual = format!("{:x}", hasher.finalize());
    if downloaded != artifact.size_bytes || actual != artifact.sha256 {
        return Err(Failure {
            code: "INTEGRITY",
            message: format!("{component} 下载文件大小或 SHA-256 不符"),
            retryable: true,
        }
        .into());
    }
    emit(component, "downloaded", downloaded, artifact.size_bytes);
    Ok(())
}

pub fn valid_file(path: &Path, size: u64, sha256: &str) -> Result<bool> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(false);
    };
    if !metadata.is_file() || metadata.len() != size {
        return Ok(false);
    }
    Ok(hash_file(path)? == sha256)
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = crate::retry::file("open file for SHA-256", path, || File::open(path))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("hash file: {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn emit(component: &str, phase: &str, downloaded: u64, total: u64) {
    println!(
        "{}",
        serde_json::json!({
            "componentId": component,
            "phase": phase,
            "downloaded": downloaded,
            "total": total
        })
    );
}
