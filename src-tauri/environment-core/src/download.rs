use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

use crate::model::EnvironmentArtifact;

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
        fs::remove_file(&destination).context("remove invalid artifact cache")?;
    }
    if artifact.url.is_empty() {
        bail!("Fusion did not return a download URL for {component}")
    }
    fs::create_dir_all(&cache_dir).context("create artifact cache")?;
    let partial = cache_dir.join("artifact.part");
    download(client, &artifact.url, &partial, artifact, component)?;
    fs::rename(&partial, &destination).context("activate verified artifact cache")?;
    Ok(destination)
}

fn download(
    client: &Client,
    url: &str,
    destination: &Path,
    artifact: &EnvironmentArtifact,
    component: &str,
) -> Result<()> {
    let mut response = client.get(url).send().context("download TOS artifact")?;
    if !response.status().is_success() {
        bail!("TOS download failed with status {}", response.status())
    }
    let mut file = File::create(destination).context("create partial artifact")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    loop {
        let read = response.read(&mut buffer).context("read TOS artifact")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .context("write partial artifact")?;
        hasher.update(&buffer[..read]);
        downloaded = downloaded.saturating_add(read as u64);
        emit(component, "downloading", downloaded, artifact.size_bytes);
        if downloaded > artifact.size_bytes {
            bail!("downloaded artifact exceeds the declared size")
        }
    }
    file.sync_all().context("flush partial artifact")?;
    let actual = format!("{:x}", hasher.finalize());
    if downloaded != artifact.size_bytes || actual != artifact.sha256 {
        bail!("downloaded artifact failed size or SHA-256 verification")
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
    let mut file = File::open(path).context("open file for SHA-256")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).context("hash file")?;
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
