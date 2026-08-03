//! 首次启动运行时的兼容入口（Node / Git）。
//!
//! 受管分发后，Node、Git、uv 等运行时优先由后端版本中心决策、短时票据 +
//! TOS/CDN 下载、本地校验后原子激活。若后端尚无对应发布数据，Node / Git
//! 可使用编译内固定版本、固定 SHA-256 的备案源完成首次启动。
//!
//! 本模块保留旧 `runtime_bootstrap` / `runtime_bootstrap_core` 表面以便现有
//! 调用方继续编译：
//!
//! - `runtime_bootstrap_core` 只做受管库存探测，缺失时给出可操作的失败信息
//!   （真正的初始化由 `bootstrap_initialize` 驱动，见 version_center installer）。
//! - `runtime_bootstrap_managed_core` 是受管安装入口（需要数据库连接），由
//!   Task 13 接线到新的 web handler / tauri command。
//!
//! 进度仍通过 `app://runtime-bootstrap` 事件上报（与旧 UI 兼容）。

use std::path::Path;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use crate::acp::version_center::managed_tool_executable;
use crate::web::event_bridge::EventEmitter;

#[cfg(feature = "tauri-runtime")]
use crate::acp::manager::ConnectionManager;

mod fallback;
mod managed;
mod types;

use types::{emit, RuntimeBootstrapEventKind};
pub use types::{RuntimeBootstrapReport, RuntimeComponentReport, RuntimeComponentStatus};

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 兼容入口：只做受管库存探测，不再执行上游下载。
pub async fn runtime_bootstrap_core(
    task_id: String,
    emitter: &EventEmitter,
) -> RuntimeBootstrapReport {
    let _guard = bootstrap_lock().lock().await;
    emit(
        emitter,
        &task_id,
        RuntimeBootstrapEventKind::Started,
        None,
        None,
        "",
    );
    let report = RuntimeBootstrapReport {
        node: probe_component("node", "node.exe"),
        git: probe_component("git", "cmd/git.exe"),
    };
    let failed = report.node.status == RuntimeComponentStatus::Failed
        || report.git.status == RuntimeComponentStatus::Failed;
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
    report
}

/// 受管安装入口：经由版本中心安装 Node / Git（resolve → 票据 → TOS 下载 →
/// 校验 → 原子激活）。Task 13 应将其接线到 web handler / tauri command。
pub async fn runtime_bootstrap_managed_core(
    conn: &DatabaseConnection,
    data_dir: &Path,
    defer_while_active: bool,
    task_id: String,
    emitter: &EventEmitter,
) -> RuntimeBootstrapReport {
    let _guard = bootstrap_lock().lock().await;
    emit(
        emitter,
        &task_id,
        RuntimeBootstrapEventKind::Started,
        None,
        None,
        "",
    );
    let channel = crate::update::preferences::load(conn)
        .await
        .map(|prefs| prefs.channel.as_str().to_string())
        .unwrap_or_else(|_| "stable".to_string());

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

    if node.status == RuntimeComponentStatus::Installed
        || git.status == RuntimeComponentStatus::Installed
    {
        crate::process::ensure_managed_tools_in_path();
    }
    let failed = node.status == RuntimeComponentStatus::Failed
        || git.status == RuntimeComponentStatus::Failed;
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
    RuntimeBootstrapReport { node, git }
}

/// 非 Windows（或未知架构）只做 PATH 探测。
fn probe_only_report(binary: &str) -> RuntimeComponentReport {
    match which::which(binary) {
        Ok(path) => RuntimeComponentReport {
            status: RuntimeComponentStatus::Ready,
            detail: Some(path.to_string_lossy().into_owned()),
        },
        Err(_) => RuntimeComponentReport {
            status: RuntimeComponentStatus::Skipped,
            detail: Some(format!("{binary} not found in PATH")),
        },
    }
}

/// 探测受管运行时是否已就绪。
fn probe_component(tool_id: &str, relative: &str) -> RuntimeComponentReport {
    if !cfg!(windows) {
        return probe_only_report(tool_id);
    }
    if let Some(path) = managed_tool_executable(tool_id) {
        return RuntimeComponentReport {
            status: RuntimeComponentStatus::Ready,
            detail: Some(path.to_string_lossy().into_owned()),
        };
    }
    let _ = relative;
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Failed,
        detail: Some(format!(
            "受管 {tool_id} 尚未安装：请先完成桌面初始化（托管分发）"
        )),
    }
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn runtime_bootstrap(
    task_id: String,
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<RuntimeBootstrapReport, String> {
    let emitter = EventEmitter::Tauri(app);
    let conn = db.conn.clone();
    let data_dir = crate::system_skills::data_dir_from_env();
    let defer_while_active = connection_manager.has_live_agent_sessions().await;
    let report =
        runtime_bootstrap_managed_core(&conn, &data_dir, defer_while_active, task_id, &emitter)
            .await;
    tauri::async_runtime::spawn(async move {
        crate::system_skills::startup_update_core(&conn, &data_dir, &emitter).await;
    });
    Ok(report)
}

/// 受管初始化状态查询（只读，不取写入锁）。供新前端 `bootstrapInitStatus`
/// 调用；web handler 等价路由由 Task 13 接线。
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn bootstrap_init_status() -> Result<crate::acp::version_center::InitStatusReport, String>
{
    let data_dir = crate::system_skills::data_dir_from_env();
    crate::acp::version_center::bootstrap_init_status(&data_dir)
        .await
        .map_err(|error| error.message)
}

/// 统一初始化 / 修复入口：resolve → 票据 → 下载 → 校验 → 激活 → health check。
/// 供新前端 `bootstrapInitialize` 调用；web handler 等价路由由 Task 13 接线。
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn bootstrap_initialize(
    task_id: String,
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
    connection_manager: tauri::State<'_, ConnectionManager>,
) -> Result<crate::acp::version_center::InitStatusReport, String> {
    let emitter = EventEmitter::Tauri(app);
    let conn = db.conn.clone();
    let data_dir = crate::system_skills::data_dir_from_env();
    let defer_while_active = connection_manager.has_live_agent_sessions().await;
    let channel = crate::update::preferences::load(&conn)
        .await
        .map(|prefs| prefs.channel.as_str().to_string())
        .unwrap_or_else(|_| "stable".to_string());
    crate::acp::version_center::bootstrap_initialize(
        &conn,
        &data_dir,
        &channel,
        defer_while_active,
        &task_id,
        &emitter,
    )
    .await
    .map_err(|error| error.message)
}
