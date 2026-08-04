//! 可续传制品下载器（Range / If-Range / `.part`）。
//!
//! 满足托管分发的下载契约：
//!
//! - `.part` 文件与 sidecar metadata 绑定 URL / 预期大小 / SHA-256 / ETag。
//! - 已存在且大小、摘要完全匹配的最终文件直接返回（零下载）。
//! - 续传使用 `Range` + `If-Range`；206 校验 `Content-Range` 与断点一致。
//! - 服务器忽略 Range 返回 200 时安全重建 part；416 重新 HEAD 判断后从头下载。
//! - 401/403 视为票据过期，返回 `AuthenticationFailed`，由调用方刷新 ticket
//!   后重试（不改变 artifact 语义）。
//! - 每次尝试有连接 / 总超时，网络抖动指数退避 + 抖动，有界重试。

use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::redirect::Policy;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Semaphore;

use crate::app_error::{AppCommandError, AppErrorCode};

pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_ATTEMPTS: u32 = 5;
const BACKOFF_BASE_MS: u64 = 1_000;
const PROGRESS_GRANULARITY: u64 = 128 * 1024;
const PROGRESS_MIN_INTERVAL_MS: u64 = 500;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// IR-008：全局并发下载上限（无依赖组件下载并行，默认小并发）。
const MAX_GLOBAL_DOWNLOADS: usize = 2;
/// IR-008：每 host 并发上限（同一 CDN/TOS 域名避免被打爆）。
const MAX_HOST_DOWNLOADS: usize = 1;

static GLOBAL_DOWNLOAD_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(MAX_GLOBAL_DOWNLOADS));

static HOST_SEMAPHORES: LazyLock<Mutex<HashMap<String, std::sync::Arc<Semaphore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .filter(|host| !host.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn host_semaphore(host: &str) -> std::sync::Arc<Semaphore> {
    let mut map = HOST_SEMAPHORES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.entry(host.to_string())
        .or_insert_with(|| std::sync::Arc::new(Semaphore::new(MAX_HOST_DOWNLOADS)))
        .clone()
}

static DOWNLOAD_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(ATTEMPT_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .map_err(|error| error.to_string())
});

/// 进度事件：已下载字节、总字节、速率（B/s）与预计剩余秒数。
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub rate_bps: u64,
    pub eta_secs: u64,
}

/// 每次网络尝试的错误分类。
enum AttemptError {
    /// 票据过期（401/403），调用方应刷新下载票据后重试。
    TicketExpired,
    /// 可重试的瞬时错误（断连、超时、5xx）。
    Transient(String),
    /// 不可重试的确定性错误。
    Fatal(AppCommandError),
}

/// 下载到 `final_path`。最终文件已完整时立即返回；否则写 `.part` 并原子改名。
pub async fn download_resumable(
    artifact_id: &str,
    url: &str,
    final_path: &Path,
    expected_size: i64,
    expected_sha256: &str,
    on_progress: Option<&(dyn Fn(DownloadProgress) + Send + Sync)>,
) -> Result<(), AppCommandError> {
    if expected_size <= 0 || expected_size as u64 > MAX_ARCHIVE_BYTES {
        return Err(AppCommandError::invalid_input(
            "Managed artifact size is outside the allowed range",
        ));
    }
    if final_matches(final_path, expected_size, expected_sha256).await {
        return Ok(());
    }

    // IR-008：全局 + 每 host 并发限制。信号量在最终文件已存在（零下载）
    // 之后才获取，避免 keep 路径占用并发额度；许可持有整个下载过程。
    let _global_permit = GLOBAL_DOWNLOAD_SEMAPHORE.acquire().await.map_err(|error| {
        AppCommandError::task_execution_failed(format!(
            "Global download concurrency gate failed: {error}"
        ))
    })?;
    let _host_permit = host_semaphore(&host_of(url))
        .acquire_owned()
        .await
        .map_err(|error| {
            AppCommandError::task_execution_failed(format!(
                "Per-host download concurrency gate failed: {error}"
            ))
        })?;

    let part_path = part_path_for(final_path);
    let meta_path = part_meta_path(final_path);
    let etag = read_resume_meta(&meta_path, artifact_id, url, expected_size, expected_sha256).await;
    if let Some(parent) = part_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppCommandError::io)?;
    }

    for attempt in 1..=MAX_ATTEMPTS {
        match attempt_once(
            url,
            &part_path,
            expected_size,
            expected_sha256,
            etag.as_deref(),
            on_progress,
        )
        .await
        {
            Ok(new_etag) => {
                if let Some(value) = new_etag {
                    let _ = tokio::fs::write(
                        &meta_path,
                        resume_meta(artifact_id, url, expected_size, expected_sha256, &value),
                    )
                    .await;
                }
                return finalize_part(&part_path, final_path, expected_size, expected_sha256)
                    .await
                    .map_err(|error| {
                        let _ = std::fs::remove_file(&part_path);
                        error
                    });
            }
            Err(AttemptError::TicketExpired) => {
                let _ = tokio::fs::remove_file(&meta_path).await;
                return Err(AppCommandError::new(
                    AppErrorCode::AuthenticationFailed,
                    "Managed artifact download ticket expired",
                ));
            }
            Err(AttemptError::Fatal(error)) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(error);
            }
            Err(AttemptError::Transient(detail)) => {
                if attempt == MAX_ATTEMPTS {
                    return Err(AppCommandError::network(
                        "Managed artifact download was interrupted",
                    )
                    .with_detail(detail));
                }
                let delay = backoff_delay(attempt);
                tracing::warn!(
                    attempt,
                    delay_ms = delay.as_millis(),
                    detail,
                    "[managed-download] transient failure, backing off"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
    Err(AppCommandError::network(
        "Managed artifact download was interrupted",
    ))
}

#[allow(clippy::too_many_arguments)]
async fn attempt_once(
    url: &str,
    part_path: &Path,
    expected_size: i64,
    expected_sha256: &str,
    etag: Option<&str>,
    on_progress: Option<&(dyn Fn(DownloadProgress) + Send + Sync)>,
) -> Result<Option<String>, AttemptError> {
    let client = DOWNLOAD_CLIENT.as_ref().map_err(|error| {
        AttemptError::Fatal(
            AppCommandError::configuration_invalid(
                "Managed artifact download client is unavailable",
            )
            .with_detail(error.clone()),
        )
    })?;

    let mut resume_from = part_size(part_path).await;
    let mut etag_state = etag.map(ToString::to_string);
    let mut restart = false;

    loop {
        let mut request = client.get(url);
        if resume_from > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
            if let Some(value) = etag_state.as_deref().filter(|value| !value.is_empty()) {
                request = request.header(reqwest::header::IF_RANGE, value);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|error| AttemptError::Transient(error.to_string()))?;
        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AttemptError::TicketExpired);
        }
        if status.is_server_error() {
            return Err(AttemptError::Transient(format!(
                "server returned HTTP {status}"
            )));
        }
        let new_etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);

        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            if resume_from == 0 {
                return Err(AttemptError::Fatal(AppCommandError::invalid_input(
                    "Managed artifact rejected the download",
                )));
            }
            let remote_size = head_content_length(client, url).await?;
            if remote_size != expected_size as u64 {
                return Err(AttemptError::Fatal(AppCommandError::invalid_input(
                    "Managed artifact size changed on the server",
                )));
            }
            // 服务器不再支持该断点：截断 part，从头全量下载。
            truncate_part(part_path).await;
            resume_from = 0;
            etag_state = None;
            restart = true;
            continue;
        }
        if !status.is_success() {
            return Err(AttemptError::Fatal(
                AppCommandError::network("Managed artifact download was rejected")
                    .with_detail(status.to_string()),
            ));
        }

        let (truncate, server_resume) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            let (start, end, total) = parse_content_range(response.headers())?;
            let expected_total = expected_size as u64;
            if start != resume_from || end != expected_total - 1 || total != expected_total {
                if restart {
                    return Err(AttemptError::Fatal(AppCommandError::invalid_input(
                        "Managed artifact Content-Range does not match the requested range",
                    )));
                }
                // 断点与服务器不一致或制品大小变化：重建 part 从头下载。
                truncate_part(part_path).await;
                resume_from = 0;
                restart = true;
                continue;
            }
            (false, resume_from)
        } else {
            // 服务器忽略 Range：截断 part，从 0 开始。
            truncate_part(part_path).await;
            (true, 0)
        };

        stream_to_part(
            response,
            part_path,
            truncate,
            server_resume,
            expected_size,
            expected_sha256,
            on_progress,
        )
        .await?;
        return Ok(new_etag);
    }
}

async fn head_content_length(client: &reqwest::Client, url: &str) -> Result<u64, AttemptError> {
    let head = client
        .head(url)
        .send()
        .await
        .map_err(|error| AttemptError::Transient(error.to_string()))?;
    head.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            AttemptError::Fatal(AppCommandError::invalid_input(
                "Managed artifact HEAD is missing Content-Length",
            ))
        })
}

#[allow(clippy::too_many_arguments)]
async fn stream_to_part(
    response: reqwest::Response,
    part_path: &Path,
    truncate: bool,
    server_resume: u64,
    expected_size: i64,
    expected_sha256: &str,
    on_progress: Option<&(dyn Fn(DownloadProgress) + Send + Sync)>,
) -> Result<(), AttemptError> {
    let (mut output, mut hasher) = prepare_part_output(part_path, truncate, server_resume).await?;
    let mut stream = response.bytes_stream();
    let mut total = server_resume;
    let started = Instant::now();
    let mut last_reported_at = Instant::now();
    let mut last_reported_bytes = server_resume;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AttemptError::Transient(error.to_string()))?;
        total = total.saturating_add(chunk.len() as u64);
        if total > expected_size as u64 || total > MAX_ARCHIVE_BYTES {
            return Err(AttemptError::Fatal(AppCommandError::invalid_input(
                "Managed artifact archive is too large",
            )));
        }
        hasher.update(&chunk);
        output.write_all(&chunk).await.map_err(io_attempt_error)?;
        if let Some(cb) = on_progress {
            let interval = last_reported_at.elapsed().as_millis() as u64;
            if total.saturating_sub(last_reported_bytes) >= PROGRESS_GRANULARITY
                || interval >= PROGRESS_MIN_INTERVAL_MS
            {
                cb(progress_event(
                    total,
                    expected_size as u64,
                    started.elapsed().as_secs_f64(),
                ));
                last_reported_at = Instant::now();
                last_reported_bytes = total;
            }
        }
    }
    if let Some(cb) = on_progress {
        cb(progress_event(
            total,
            expected_size as u64,
            started.elapsed().as_secs_f64(),
        ));
    }
    output.flush().await.map_err(io_attempt_error)?;
    output.sync_all().await.map_err(io_attempt_error)?;
    if total != expected_size as u64
        || !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected_sha256)
    {
        return Err(AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact integrity check failed",
        )));
    }
    Ok(())
}

async fn prepare_part_output(
    part_path: &Path,
    truncate: bool,
    server_resume: u64,
) -> Result<(tokio::fs::File, Sha256), AttemptError> {
    let mut output = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(truncate)
        .open(part_path)
        .await
        .map_err(io_attempt_error)?;
    let mut hasher = Sha256::new();
    if server_resume == 0 {
        return Ok((output, hasher));
    }

    let actual_size = output.metadata().await.map_err(io_attempt_error)?.len();
    if actual_size != server_resume {
        return Err(AttemptError::Fatal(AppCommandError::io_error(
            "Partial artifact changed while resuming",
        )));
    }

    output
        .seek(SeekFrom::Start(0))
        .await
        .map_err(io_attempt_error)?;
    let mut remaining = server_resume;
    // 缓冲区会跨 `.await` 留在多层 future 中，必须放到堆上，避免撑爆
    // Tauri 的 Windows 主线程栈。
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    while remaining > 0 {
        let read_size = remaining.min(HASH_BUFFER_BYTES as u64) as usize;
        output
            .read_exact(&mut buffer[..read_size])
            .await
            .map_err(io_attempt_error)?;
        hasher.update(&buffer[..read_size]);
        remaining -= read_size as u64;
    }
    output
        .seek(SeekFrom::Start(server_resume))
        .await
        .map_err(io_attempt_error)?;
    Ok((output, hasher))
}

fn io_attempt_error(error: std::io::Error) -> AttemptError {
    AttemptError::Fatal(AppCommandError::io(error))
}

async fn finalize_part(
    part_path: &Path,
    final_path: &Path,
    expected_size: i64,
    expected_sha256: &str,
) -> Result<(), AppCommandError> {
    if part_size(part_path).await != expected_size as u64 {
        return Err(AppCommandError::invalid_input(
            "Managed artifact download is incomplete",
        ));
    }
    let path = part_path.to_path_buf();
    let expected = expected_sha256.to_string();
    let matches = tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).ok()?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).ok()?;
        Some(format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected))
    })
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?
    .unwrap_or(false);
    if !matches {
        return Err(AppCommandError::invalid_input(
            "Managed artifact integrity check failed",
        ));
    }
    tokio::fs::rename(part_path, final_path)
        .await
        .map_err(AppCommandError::io)
}

/// 最终文件已存在且大小、摘要匹配 → 零下载。
async fn final_matches(path: &Path, expected_size: i64, expected_sha256: &str) -> bool {
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return false;
    };
    if !metadata.is_file() || metadata.len() != expected_size as u64 {
        return false;
    }
    let path = path.to_path_buf();
    let expected = expected_sha256.to_string();
    tokio::task::spawn_blocking(move || {
        let Ok(mut file) = std::fs::File::open(&path) else {
            return false;
        };
        let mut hasher = Sha256::new();
        if std::io::copy(&mut file, &mut hasher).is_err() {
            return false;
        }
        format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected)
    })
    .await
    .unwrap_or(false)
}

async fn part_size(path: &Path) -> u64 {
    tokio::fs::metadata(path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

async fn truncate_part(path: &Path) {
    if let Ok(mut file) = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await
    {
        let _ = file.flush().await;
    }
}

fn part_path_for(final_path: &Path) -> PathBuf {
    let mut name = final_path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    name.push_str(".part");
    final_path.with_file_name(name)
}

fn part_meta_path(final_path: &Path) -> PathBuf {
    let mut name = final_path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    name.push_str(".part.meta.json");
    final_path.with_file_name(name)
}

fn resume_meta(
    artifact_id: &str,
    url: &str,
    expected_size: i64,
    expected_sha256: &str,
    etag: &str,
) -> Vec<u8> {
    serde_json::json!({
        "artifact_id": artifact_id,
        "url": url,
        "expected_size": expected_size,
        "expected_sha256": expected_sha256,
        "etag": etag,
    })
    .to_string()
    .into_bytes()
}

/// 读取并校验 sidecar metadata。仅当 artifact ID / URL / 大小 / 摘要全部匹配时
/// 才返回可续传的 ETag；任何不匹配都丢弃旧 metadata，从头下载。
async fn read_resume_meta(
    meta_path: &Path,
    artifact_id: &str,
    url: &str,
    expected_size: i64,
    expected_sha256: &str,
) -> Option<String> {
    let raw = tokio::fs::read_to_string(meta_path).await.ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    let matches = value.get("artifact_id")?.as_str() == Some(artifact_id)
        && value.get("url")?.as_str() == Some(url)
        && value.get("expected_size")?.as_i64() == Some(expected_size)
        && value
            .get("expected_sha256")?
            .as_str()
            .is_some_and(|sha| sha.eq_ignore_ascii_case(expected_sha256));
    if !matches {
        let _ = tokio::fs::remove_file(meta_path).await;
        return None;
    }
    value.get("etag")?.as_str().map(ToString::to_string)
}

fn parse_content_range(
    headers: &reqwest::header::HeaderMap,
) -> Result<(u64, u64, u64), AttemptError> {
    let value = headers
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            AttemptError::Fatal(AppCommandError::invalid_input(
                "Managed artifact response is missing Content-Range",
            ))
        })?;
    // 格式：bytes <start>-<end>/<total>
    let rest = value.strip_prefix("bytes ").ok_or_else(|| {
        AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact Content-Range is malformed",
        ))
    })?;
    let (range, total) = rest.rsplit_once('/').ok_or_else(|| {
        AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact Content-Range is malformed",
        ))
    })?;
    let total = total.parse::<u64>().map_err(|_| {
        AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact Content-Range total is invalid",
        ))
    })?;
    let (start, end) = range.split_once('-').ok_or_else(|| {
        AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact Content-Range range is malformed",
        ))
    })?;
    let start = start.parse::<u64>().map_err(|_| {
        AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact Content-Range start is invalid",
        ))
    })?;
    let end = end.parse::<u64>().map_err(|_| {
        AttemptError::Fatal(AppCommandError::invalid_input(
            "Managed artifact Content-Range end is invalid",
        ))
    })?;
    Ok((start, end, total))
}

fn progress_event(downloaded: u64, total: u64, elapsed_secs: f64) -> DownloadProgress {
    let rate_bps = if elapsed_secs > 0.0 {
        (downloaded as f64 / elapsed_secs) as u64
    } else {
        0
    };
    let eta_secs = if rate_bps > 0 && downloaded < total {
        (total - downloaded) as u64 / rate_bps.max(1)
    } else {
        0
    };
    DownloadProgress {
        downloaded,
        total,
        rate_bps,
        eta_secs,
    }
}

fn backoff_delay(attempt: u32) -> Duration {
    let base = BACKOFF_BASE_MS.saturating_mul(1_u64 << (attempt - 1).min(4));
    let jitter = (attempt as u64).wrapping_mul(0x9E37_79B9) % 500;
    Duration::from_millis(base + jitter)
}
