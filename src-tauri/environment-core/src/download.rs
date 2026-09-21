use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use reqwest::blocking::Response;
use sha2::{Digest, Sha256};

use crate::client::FusionClient;
use crate::failure::{self, Failure};
use crate::model::{EnvironmentAction, EnvironmentArtifact, ResolveRequest};

const BUFFER_BYTES: usize = 128 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

struct Transfer<'a> {
    component: &'a str,
    offset: u64,
    total: u64,
}

pub struct CacheRequest<'a> {
    pub client: &'a FusionClient,
    pub environment: &'a ResolveRequest,
    pub cache_root: &'a Path,
    pub action: &'a EnvironmentAction,
}

pub fn ensure_cached(request: CacheRequest<'_>) -> Result<PathBuf> {
    let artifact = &request.action.artifact;
    let component = &request.action.component_id;
    let cache_dir = request.cache_root.join(&artifact.sha256);
    let destination = cache_dir.join("artifact");
    if valid_file(&destination, artifact.size_bytes, &artifact.sha256)? {
        emit(
            component,
            "cached",
            (artifact.size_bytes, artifact.size_bytes),
        );
        return Ok(destination);
    }
    if destination.exists() {
        fs::remove_file(&destination)?;
    }
    fs::create_dir_all(&cache_dir).context("无法创建下载缓存，请检查磁盘权限")?;
    let partial = cache_dir.join("artifact.part");
    crate::retry::run(component, || {
        let refreshed = request
            .client
            .refresh(request.environment, request.action)?;
        download(&request, &partial, &refreshed)
    })?;
    fs::rename(&partial, &destination).context("无法保存已校验的下载文件")?;
    Ok(destination)
}

fn download(
    request: &CacheRequest<'_>,
    destination: &Path,
    artifact: &EnvironmentArtifact,
) -> Result<()> {
    if valid_file(destination, artifact.size_bytes, &artifact.sha256)? {
        return Ok(());
    }
    let mut offset = fs::metadata(destination).map_or(0, |metadata| metadata.len());
    if offset >= artifact.size_bytes {
        fs::remove_file(destination)?;
        offset = 0;
    }
    let mut http = request.client.http().get(&artifact.url);
    if offset > 0 {
        http = http.header("Range", format!("bytes={offset}-"));
    }
    let response = http.send().map_err(failure::network)?;
    let offset = check_response(&response, artifact.size_bytes, offset)?;
    let total = write_response(
        response,
        destination,
        Transfer {
            component: &request.action.component_id,
            offset,
            total: artifact.size_bytes,
        },
    )?;
    if total != artifact.size_bytes || hash_file(destination)? != artifact.sha256 {
        fs::remove_file(destination).context("移除损坏的下载片段失败")?;
        return Err(Failure {
            code: "INTEGRITY",
            retryable: true,
            message: "下载文件的大小或 SHA-256 不符，将重新下载；请检查代理或网络缓存".into(),
        }
        .into());
    }
    emit(&request.action.component_id, "downloaded", (total, total));
    Ok(())
}

fn check_response(response: &Response, size: u64, offset: u64) -> Result<u64> {
    let status = response.status().as_u16();
    if status == 206 && offset > 0 {
        let expected = format!("bytes {offset}-{}/{size}", size - 1);
        if response
            .headers()
            .get("Content-Range")
            .and_then(|v| v.to_str().ok())
            != Some(&expected)
        {
            return Err(Failure::permanent("INTEGRITY", "TOS 断点响应范围不符").into());
        }
        return Ok(offset);
    }
    if status == 200 {
        return Ok(0);
    }
    let message = format!("TOS 下载失败（HTTP {status}），请检查网络或稍后重试");
    Err(Failure {
        code: "NETWORK",
        message,
        retryable: matches!(status, 403 | 408 | 429 | 500..=599),
    }
    .into())
}

fn write_response(mut response: Response, path: &Path, progress: Transfer<'_>) -> Result<u64> {
    let Transfer {
        component,
        offset,
        total,
    } = progress;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(path)
        .context("无法写入下载文件，请检查磁盘空间和权限")?;
    let mut buffer = [0_u8; BUFFER_BYTES];
    let mut downloaded = offset;
    let mut last_event = Instant::now();
    emit(component, "downloading", (downloaded, total));
    loop {
        let read = response.read(&mut buffer).map_err(|error| {
            Failure::network(format!(
                "下载连接中断（{:?}），将从已下载位置重试",
                error.kind()
            ))
        })?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .context("download size overflow")?;
        if downloaded > total {
            bail!("download exceeds the Fusion artifact size")
        }
        file.write_all(&buffer[..read])
            .context("写入失败，请检查磁盘空间和权限")?;
        if last_event.elapsed() >= PROGRESS_INTERVAL {
            emit(component, "downloading", (downloaded, total));
            last_event = Instant::now();
        }
    }
    file.sync_all().context("刷新下载文件失败")?;
    Ok(downloaded)
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
    let mut buffer = [0_u8; BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).context("hash file")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn emit(component: &str, phase: &str, progress: (u64, u64)) {
    let (downloaded, total) = progress;
    if !std::env::args().any(|argument| argument == "--json") {
        println!("{component}: {phase} ({downloaded}/{total})");
        return;
    }
    println!(
        "{}",
        serde_json::json!({
            "componentId": component, "phase": phase, "downloaded": downloaded, "total": total
        })
    );
}
