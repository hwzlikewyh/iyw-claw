use std::io;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

use crate::web::event_bridge::{emit_event, EventEmitter};

const REPAIR_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const ERROR_LIMIT: usize = 2_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    component_id: String,
    phase: String,
    #[serde(default)]
    percent: Option<u32>,
    #[serde(default)]
    downloaded: u64,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    message: String,
}

struct RepairCacheGuard;

impl Drop for RepairCacheGuard {
    fn drop(&mut self) {
        super::snapshot::clear();
        super::verification_cache::clear();
    }
}

pub async fn repair(task_id: &str, emitter: &EventEmitter) -> Result<(), String> {
    super::snapshot::clear();
    super::verification_cache::clear();
    let _cache_guard = RepairCacheGuard;
    tokio::time::timeout(REPAIR_TIMEOUT, run(task_id, emitter))
        .await
        .map_err(|_| "环境修复超时，请检查网络后重试".to_string())?
}

async fn run(task_id: &str, emitter: &EventEmitter) -> Result<(), String> {
    let helper = super::environment_helper()
        .ok_or_else(|| "安装目录缺少环境修复程序，请重新安装应用".to_string())?;
    let mut child = crate::process::tokio_command(helper)
        .args([
            "repair",
            "--app-version",
            env!("CARGO_PKG_VERSION"),
            "--json",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("无法启动环境修复程序：{error}"))?;
    let stdout = child.stdout.take().ok_or("无法读取修复进度")?;
    let stderr = child.stderr.take().ok_or("无法读取修复错误")?;
    let (progress, detail, status) = tokio::join!(
        forward_progress(stdout, task_id, emitter),
        error_tail(stderr),
        child.wait(),
    );
    let status = status.map_err(|error| format!("环境修复进程异常：{error}"))?;
    let detail = detail.map_err(|error| format!("无法读取修复错误：{error}"))?;
    if !status.success() {
        return Err(format!("环境修复失败（{status}）：{detail}"));
    }
    progress.map_err(|error| format!("无法读取环境修复进度：{error}"))
}

async fn forward_progress(
    stream: impl AsyncRead + Unpin,
    task_id: &str,
    emitter: &EventEmitter,
) -> io::Result<()> {
    let mut lines = BufReader::new(stream).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<Progress>(&line) else {
            continue;
        };
        let component = (event.component_id != "environment").then_some(event.component_id);
        emit_event(
            emitter,
            "app://bootstrap-init",
            serde_json::json!({
                "taskId": task_id, "phase": bootstrap_phase(&event.phase), "component": component,
                "percent": event.percent,
                "downloaded": event.downloaded, "total": event.total, "message": event.message,
            }),
        );
    }
    Ok(())
}

fn bootstrap_phase(phase: &str) -> &str {
    match phase {
        "downloaded" | "cached" | "extracting" | "reused" | "component-prepared"
        | "optional-skipped" | "prepared" => "staging",
        "verified" => "health_check",
        "committed" => "ready",
        phase => phase,
    }
}

async fn error_tail(mut stream: impl AsyncRead + Unpin) -> io::Result<String> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        result.extend_from_slice(&buffer[..count]);
        if result.len() > ERROR_LIMIT {
            result.drain(..result.len() - ERROR_LIMIT);
        }
    }
    Ok(String::from_utf8_lossy(&result).trim().to_string())
}
