use std::io;
use std::process::Stdio;
use std::time::{Duration, Instant};

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

struct ProgressSink<'a> {
    task_id: &'a str,
    emitter: &'a EventEmitter,
    on_progress: &'a (dyn Fn(&str) + Send + Sync),
}

struct RepairCacheGuard;

impl Drop for RepairCacheGuard {
    fn drop(&mut self) {
        super::snapshot::clear();
        super::verification_cache::clear();
    }
}

pub async fn repair_startup(
    task_id: &str,
    emitter: &EventEmitter,
    explicit_repair: bool,
) -> Result<(), String> {
    let Err(error) = repair_with_progress(task_id, emitter, |_| {}).await else {
        return Ok(());
    };
    if !explicit_repair {
        let report = tokio::task::spawn_blocking(super::init_status_report)
            .await
            .map_err(|probe_error| format!("{error}；环境复检任务异常：{probe_error}"))?;
        if !super::core_components_need_repair(&report) {
            tracing::warn!(
                task_id,
                error,
                "optional environment repair failed; core components still ready"
            );
            return Ok(());
        }
    }
    Err(error)
}

pub async fn repair_with_progress(
    task_id: &str,
    emitter: &EventEmitter,
    on_progress: impl Fn(&str) + Send + Sync,
) -> Result<(), String> {
    let started = Instant::now();
    super::snapshot::clear();
    super::verification_cache::clear();
    let _cache_guard = RepairCacheGuard;
    tracing::info!(task_id, "environment repair started");
    let sink = ProgressSink {
        task_id,
        emitter,
        on_progress: &on_progress,
    };
    let result = tokio::time::timeout(REPAIR_TIMEOUT, run(sink, "repair"))
        .await
        .map_err(|_| "环境修复超时，请检查网络后重试".to_string())
        .and_then(|result| result);
    match &result {
        Ok(()) => tracing::info!(
            task_id,
            duration_ms = started.elapsed().as_millis() as u64,
            "environment repair completed"
        ),
        Err(error) => tracing::error!(task_id, error, "environment repair failed"),
    }
    result
}

pub async fn change_computer_driver(
    install: bool,
    emitter: &EventEmitter,
    on_progress: impl Fn(&str) + Send + Sync,
) -> Result<(), String> {
    let _writer = super::lock_writer().await;
    super::snapshot::clear();
    super::verification_cache::clear();
    let _cache_guard = RepairCacheGuard;
    let command = if install {
        "install-computer-driver"
    } else {
        "remove-computer-driver"
    };
    let sink = ProgressSink {
        task_id: command,
        emitter,
        on_progress: &on_progress,
    };
    tracing::info!(install, "[computer] managed driver change started");
    let result = tokio::time::timeout(REPAIR_TIMEOUT, run(sink, command))
        .await
        .map_err(|_| "驱动组件操作超时".to_string())
        .and_then(|result| result);
    if let Err(error) = &result {
        tracing::error!(install, error, "[computer] managed driver change failed");
    }
    result
}

async fn run(sink: ProgressSink<'_>, command: &str) -> Result<(), String> {
    let helper = super::environment_helper()
        .ok_or_else(|| "安装目录缺少环境修复程序，请重新安装应用".to_string())?;
    let mut child = crate::process::tokio_command(helper)
        .args([
            command,
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
        forward_progress(stdout, &sink),
        error_tail(stderr),
        child.wait(),
    );
    let status = status.map_err(|error| format!("环境修复进程异常：{error}"))?;
    let detail = detail.map_err(|error| format!("无法读取修复错误：{error}"))?;
    if !detail.is_empty() {
        if status.success() {
            tracing::warn!(
                task_id = sink.task_id,
                diagnostic = %detail,
                "environment repair completed with helper diagnostics"
            );
        }
        (sink.on_progress)(&detail);
    }
    if !status.success() {
        return Err(format!("环境修复失败（{status}）：{detail}"));
    }
    progress.map_err(|error| format!("无法读取环境修复进度：{error}"))
}

async fn forward_progress(
    stream: impl AsyncRead + Unpin,
    sink: &ProgressSink<'_>,
) -> io::Result<()> {
    let mut lines = BufReader::new(stream).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<Progress>(&line) else {
            continue;
        };
        if event.phase == "optional-skipped" {
            tracing::warn!(
                task_id = sink.task_id,
                component_id = %event.component_id,
                "optional environment component skipped during repair"
            );
        }
        let percent = event
            .percent
            .map(|value| format!(" ({value}%)"))
            .unwrap_or_default();
        (sink.on_progress)(&format!("{}: {}{percent}", event.component_id, event.phase));
        let component = (event.component_id != "environment").then_some(event.component_id);
        emit_event(
            sink.emitter,
            "app://bootstrap-init",
            serde_json::json!({
                "taskId": sink.task_id, "phase": bootstrap_phase(&event.phase), "component": component,
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
