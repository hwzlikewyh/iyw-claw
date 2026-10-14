use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::web::event_bridge::EventEmitter;

#[path = "repair_progress.rs"]
mod progress;
use progress::{error_tail, forward_progress, ProgressSink};

const REPAIR_TIMEOUT: Duration = Duration::from_secs(30 * 60);

struct RepairCacheGuard;

pub struct RepairRequest<'a> {
    pub task_id: &'a str,
    pub component: Option<&'a str>,
}

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
    let request = RepairRequest {
        task_id,
        component: None,
    };
    let Err(error) = repair_with_progress(request, emitter, |_| {}).await else {
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
    request: RepairRequest<'_>,
    emitter: &EventEmitter,
    on_progress: impl Fn(&str) + Send + Sync,
) -> Result<(), String> {
    let started = Instant::now();
    super::snapshot::clear();
    super::verification_cache::clear();
    let _cache_guard = RepairCacheGuard;
    let task_id = request.task_id;
    let components = repair_selection(&request).await?;
    if components.as_deref() == Some("") {
        tracing::info!(
            task_id,
            "environment components already healthy; repair skipped"
        );
        return Ok(());
    }
    tracing::info!(task_id, ?components, "environment repair started");
    let sink = ProgressSink {
        task_id,
        emitter,
        on_progress: &on_progress,
        components: components.as_deref(),
    };
    let result = tokio::time::timeout(REPAIR_TIMEOUT, run(sink, "repair", components.as_deref()))
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

async fn repair_selection(request: &RepairRequest<'_>) -> Result<Option<String>, String> {
    let report = tokio::task::spawn_blocking(super::init_status_report)
        .await
        .map_err(|error| format!("环境检测任务异常：{error}"))?;
    if report
        .components
        .iter()
        .any(|item| item.component_id == "environment")
    {
        // 清单缺失或版本不匹配时无法安全复用，沿用完整环境恢复入口。
        if request.component.is_some_and(|id| matches!(id, "chromix" | "agent-browser")) {
            return Ok(Some("chromix,agent-browser".to_string()));
        }
        return Ok(None);
    }
    let mut components = report
        .components
        .iter()
        .filter(|item| !item.active && item.component_id != "builtin-agent")
        .map(|item| item.component_id.clone())
        .collect::<Vec<_>>();
    if request
        .component
        .is_some_and(|component| matches!(component, "chromix" | "agent-browser"))
    {
        for component in ["chromix", "agent-browser"] {
            if !components.iter().any(|id| id == component) {
                components.push(component.to_string());
            }
        }
    }
    if let Some(component) = request.component {
        if !components.iter().any(|id| id == component) {
            components.push(component.to_string());
        }
    }
    Ok(Some(components.join(",")))
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
        components: None,
    };
    tracing::info!(install, "[computer] managed driver change started");
    let result = tokio::time::timeout(REPAIR_TIMEOUT, run(sink, command, None))
        .await
        .map_err(|_| "驱动组件操作超时".to_string())
        .and_then(|result| result);
    if let Err(error) = &result {
        tracing::error!(install, error, "[computer] managed driver change failed");
    }
    result
}

async fn run(
    sink: ProgressSink<'_>,
    command: &str,
    components: Option<&str>,
) -> Result<(), String> {
    let mut child = spawn_helper(command, components)?;
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

fn spawn_helper(command: &str, components: Option<&str>) -> Result<tokio::process::Child, String> {
    let helper = super::environment_helper()
        .ok_or_else(|| "安装目录缺少环境修复程序，请重新安装应用".to_string())?;
    let mut process = crate::process::tokio_command(helper);
    process.args([
        command,
        "--app-version",
        env!("CARGO_PKG_VERSION"),
        "--json",
    ]);
    if let Some(components) = components {
        process.args(["--components", components]);
    }
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("无法启动环境修复程序：{error}"))
}
