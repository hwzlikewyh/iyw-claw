//! 首次启动运行时的兼容入口（Node / Git / uv）。
//!
//! 受管分发后，Node、Git、uv 等运行时优先由后端版本中心决策、短时票据 +
//! TOS/CDN 下载、本地校验后原子激活。若后端尚无对应发布数据，Node / Git / uv
//! 可使用编译内固定版本、固定 SHA-256 的备案源完成首次启动。
//!
//! 本模块保留旧 `runtime_bootstrap` / `runtime_bootstrap_core` 表面以便现有
//! 调用方继续编译：
//!
//! - `runtime_bootstrap_core` 只做受管库存探测，缺失时给出可操作的失败信息
//!   （真正的初始化由 `bootstrap_initialize` 驱动，见 version_center installer）。
//! - `runtime_bootstrap_managed_core` 是 web handler / Tauri command 共用的受管安装入口。
//!
//! 进度仍通过 `app://runtime-bootstrap` 事件上报（与旧 UI 兼容）。

use std::path::Path;
use std::time::Instant;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use crate::web::event_bridge::EventEmitter;

#[cfg(feature = "tauri-runtime")]
use crate::acp::manager::ConnectionManager;

pub(crate) mod fallback;
mod managed;
mod types;

use types::{emit, RuntimeBootstrapEventKind};
pub use types::{RuntimeBootstrapReport, RuntimeComponentReport, RuntimeComponentStatus};

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 兼容入口：只做受管库存探测，不再执行上游下载。
#[tracing::instrument(
    name = "runtime_bootstrap_core",
    skip(emitter),
    fields(
        task_id = %task_id,
        tool_id = "all",
        channel = "n/a",
        defer_while_active = false
    )
)]
pub async fn runtime_bootstrap_core(
    task_id: String,
    emitter: &EventEmitter,
) -> RuntimeBootstrapReport {
    let started = Instant::now();
    let _guard = bootstrap_lock().lock().await;
    tracing::info!(phase = "begin", "runtime bootstrap inventory probe started");
    emit(
        emitter,
        &task_id,
        RuntimeBootstrapEventKind::Started,
        None,
        None,
        "",
    );
    let report = RuntimeBootstrapReport {
        node: managed::probe_component("node"),
        git: managed::probe_component("git"),
        uv: managed::probe_component("uv"),
    };
    let failed = report.node.status == RuntimeComponentStatus::Failed
        || report.git.status == RuntimeComponentStatus::Failed
        || report.uv.status == RuntimeComponentStatus::Failed;
    emit(
        emitter,
        &task_id,
        if failed {
            RuntimeBootstrapEventKind::Failed
        } else {
            RuntimeBootstrapEventKind::Completed
        },
        None,
        None,
        "",
    );
    tracing::info!(
        phase = "end",
        outcome = if failed { "failed" } else { "completed" },
        node_status = ?report.node.status,
        git_status = ?report.git.status,
        uv_status = ?report.uv.status,
        duration_ms = started.elapsed().as_millis() as u64,
        "runtime bootstrap inventory probe finished"
    );
    report
}

/// 受管安装入口：经由版本中心安装 Node / Git / uv（resolve → 票据 → TOS 下载 →
/// 校验 → 原子激活）。
#[tracing::instrument(
    name = "runtime_bootstrap_managed_core",
    skip_all,
    fields(
        task_id = %task_id,
        tool_id = "all",
        channel = tracing::field::Empty,
        defer_while_active = defer_while_active
    )
)]
pub async fn runtime_bootstrap_managed_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    defer_while_active: bool,
    task_id: String,
    emitter: &EventEmitter,
) -> RuntimeBootstrapReport {
    let started = Instant::now();
    let _guard = bootstrap_lock().lock().await;
    tracing::info!(phase = "begin", "managed runtime bootstrap started");
    emit(
        emitter,
        &task_id,
        RuntimeBootstrapEventKind::Started,
        None,
        None,
        "",
    );
    let channel = managed::load_channel(conn, &task_id).await;
    tracing::Span::current().record("channel", tracing::field::display(&channel));

    let node = managed::ensure_component(
        conn,
        data_dir,
        "node",
        &channel,
        defer_while_active,
        &task_id,
        emitter,
    )
    .await;
    let git = managed::ensure_component(
        conn,
        data_dir,
        "git",
        &channel,
        defer_while_active,
        &task_id,
        emitter,
    )
    .await;
    let uv = managed::ensure_component(
        conn,
        data_dir,
        "uv",
        &channel,
        defer_while_active,
        &task_id,
        emitter,
    )
    .await;

    let failed = node.status == RuntimeComponentStatus::Failed
        || git.status == RuntimeComponentStatus::Failed
        || uv.status == RuntimeComponentStatus::Failed;
    emit(
        emitter,
        &task_id,
        if failed {
            RuntimeBootstrapEventKind::Failed
        } else {
            RuntimeBootstrapEventKind::Completed
        },
        None,
        None,
        "",
    );
    tracing::info!(
        phase = "end",
        outcome = if failed { "failed" } else { "completed" },
        channel = %channel,
        node_status = ?node.status,
        git_status = ?git.status,
        uv_status = ?uv.status,
        duration_ms = started.elapsed().as_millis() as u64,
        "managed runtime bootstrap finished"
    );
    RuntimeBootstrapReport { node, git, uv }
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn runtime_bootstrap(
    task_id: String,
    app: tauri::AppHandle,
    _db: tauri::State<'_, crate::db::AppDatabase>,
    _connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<RuntimeBootstrapReport, String> {
    let emitter = EventEmitter::Tauri(app);
    Ok(runtime_bootstrap_core(task_id, &emitter).await)
}

/// 受管初始化状态查询（只读，不取写入锁）。供前端 `bootstrapInitStatus` 调用。
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn bootstrap_init_status() -> Result<crate::acp::version_center::InitStatusReport, String>
{
    Ok(crate::managed_environment::init_status_report())
}

/// 统一初始化 / 修复入口：resolve → 票据 → 下载 → 校验 → 激活 → health check。
/// 供前端 `bootstrapInitialize` 调用。
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn bootstrap_initialize(
    task_id: String,
    repair: Option<bool>,
    _app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
    _connection_manager: tauri::State<'_, ConnectionManager>,
    user_memory: tauri::State<'_, std::sync::Arc<crate::user_memory::UserMemoryService>>,
) -> Result<crate::acp::version_center::InitStatusReport, String> {
    if repair.unwrap_or(false) {
        crate::managed_environment::repair().await?;
    }
    let conn = db.conn.clone();
    let data_dir = crate::system_skills::data_dir_from_env();
    let channel = managed::load_channel(&conn, &task_id).await;
    let report = crate::managed_environment::init_status_report();
    if report.phase == "ready" {
        let service = user_memory.inner().clone();
        let model_data_dir = data_dir.clone();
        let model_channel = channel.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = service
                .prepare_managed_model(&model_data_dir, &model_channel)
                .await
            {
                tracing::info!(
                    error_code = ?error.code,
                    "[memory-model] bootstrap preparation continues in background"
                );
            }
        });
    }
    Ok(report)
}
