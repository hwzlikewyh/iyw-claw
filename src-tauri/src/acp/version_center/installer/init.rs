//! 统一初始化与修复入口（状态机编排层）。
//!
//! 首次初始化和后续修复都消费后端版本决策（resolve → 短时票据 → TOS/CDN 下载），
//! 不再直连上游。本模块负责：
//!
//! - 单实例写入锁与持久化检查点（`state` 模块）。
//! - 按组件顺序 resolve → 下载 → 校验 → 解压 → 激活 → health check。
//! - 已安装且 marker 完全匹配的版本返回 keep（零下载）。
//! - 失败分类：有健康库存 → degraded offline；缺必要组件 → blocked。
//! - 进度通过 `app://bootstrap-init` 事件上报（阶段、组件、字节、速率、ETA）。
//!
//! 后端统一 bootstrap plan 契约冻结前，本模块按现有
//! `/agent-platforms/v1/*` 契约逐组件 resolve；plan 接入点见 handoff 的
//! integration_request。

use std::path::Path;

use sea_orm::DatabaseConnection;
use serde::Serialize;

use super::bootstrap_commit::mark_component_failed;
use super::bootstrap_component::prepare_tool_components;
use super::bootstrap_finalize::commit_prepared_components;
use super::manifest::{
    active_versions, digest_managed_root, read_manifest, read_pending_activations,
    InventoryManifest, PendingActivation,
};
use super::migration::{migration_receipt, run_legacy_migration};
use super::resumable::DownloadProgress;
use super::state::{acquire_writer_lock, read_state, write_state, BootstrapState, InitPhase};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::{emit_event, EventEmitter};

pub const BOOTSTRAP_INIT_EVENT: &str = "app://bootstrap-init";
const TOOL_COMPONENTS: [&str; 3] = ["node", "git", "uv"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapInitEvent {
    pub task_id: String,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_bps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_secs: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatusView {
    pub component_id: String,
    pub component_kind: String,
    pub version: String,
    pub installed: bool,
    pub active: bool,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitStatusReport {
    pub phase: String,
    pub components: Vec<ComponentStatusView>,
    pub offline: bool,
    pub writer_busy: bool,
    pub pending_activations: Vec<PendingActivation>,
    pub manifest_generation: u64,
    pub digest: String,
    pub migrated: bool,
}

/// 初始化状态查询（只读，不取写入锁）。供新 `bootstrapInitStatus` 命令调用。
pub async fn bootstrap_init_status(data_dir: &Path) -> Result<InitStatusReport, AppCommandError> {
    let state = read_state(data_dir).await?;
    let manifest = read_manifest(data_dir).await?;
    let digest = digest_managed_root(data_dir).await?;
    let pending = read_pending_activations(data_dir).await?;
    let migrated = migration_receipt(data_dir).await?.is_some();
    Ok(InitStatusReport {
        phase: phase_label(state.phase).to_string(),
        components: state
            .components
            .into_iter()
            .map(|checkpoint| ComponentStatusView {
                component_id: checkpoint.component_id,
                component_kind: checkpoint.component_kind,
                version: checkpoint.version,
                installed: checkpoint.installed,
                active: checkpoint.active,
                phase: phase_label(checkpoint.phase).to_string(),
                last_error: checkpoint.last_error,
            })
            .collect(),
        offline: state.phase == InitPhase::Degraded,
        writer_busy: false,
        pending_activations: pending,
        manifest_generation: manifest.generation,
        digest,
        migrated,
    })
}

/// 统一初始化 / 修复入口。需要数据库连接（版本决策与票据来自后端）。
/// 供新 `bootstrapInitialize` 命令（tauri + web handler）由 Task 13 接线调用。
pub async fn bootstrap_initialize(
    conn: &DatabaseConnection,
    data_dir: &Path,
    channel: &str,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<InitStatusReport, AppCommandError> {
    let Some(_guard) = acquire_writer_lock(data_dir).await? else {
        emit_init_event(
            emitter,
            task_id,
            "retryable",
            None,
            "另一个窗口正在进行初始化，本窗口只订阅进度",
        );
        return bootstrap_init_status(data_dir).await.map(|mut report| {
            report.writer_busy = true;
            report
        });
    };

    // 一次性旧目录迁移（幂等 receipt）。
    let _migrated = run_legacy_migration(data_dir).await?.receipt_written;

    let mut state = read_state(data_dir).await?;
    if state.phase == InitPhase::Ready && components_all_ready(&state) {
        return bootstrap_init_status(data_dir).await;
    }

    state.set_phase(InitPhase::Resolving);
    write_state(data_dir, &state).await?;
    emit_init_event(emitter, task_id, "resolving", None, "");

    let mut manifest = read_manifest(data_dir).await?;
    let active = active_versions(&manifest);

    // 准备阶段不写 inventory、manifest 或 active pointer，可以并行 resolve、
    // 取票、下载、校验和解压；提交仍按固定顺序串行，保留单写入者语义。
    let [node, git, uv] = prepare_tool_components(
        conn,
        data_dir,
        channel,
        defer_while_active,
        task_id,
        emitter,
        &active,
    )
    .await;
    let mut prepared = [("node", Some(node)), ("git", Some(git)), ("uv", Some(uv))];
    let deferred_components = match commit_prepared_components(
        conn,
        data_dir,
        &mut manifest,
        &mut state,
        &mut prepared,
        defer_while_active,
        task_id,
        emitter,
    )
    .await
    {
        Ok(components) => components,
        Err((tool_id, error)) => {
            mark_component_failed(data_dir, &mut state, &manifest, &tool_id, &error).await?;
            emit_init_event(
                emitter,
                task_id,
                phase_label(state.phase),
                Some(&tool_id),
                &error.message,
            );
            return bootstrap_init_status(data_dir).await;
        }
    };

    state.set_phase(InitPhase::Ready);
    write_state(data_dir, &state).await?;
    if deferred_components.is_empty() {
        emit_init_event(emitter, task_id, "ready", None, "");
    } else {
        emit_init_event(
            emitter,
            task_id,
            "ready",
            None,
            &format!(
                "{} 已安装但激活延迟（存在活跃会话），会话结束后的首次启动将自动激活",
                deferred_components.join(", ")
            ),
        );
    }
    bootstrap_init_status(data_dir).await
}

pub(super) fn emit_init_event(
    emitter: &EventEmitter,
    task_id: &str,
    phase: &str,
    component: Option<&str>,
    message: &str,
) {
    emit_event(
        emitter,
        BOOTSTRAP_INIT_EVENT,
        BootstrapInitEvent {
            task_id: task_id.to_string(),
            phase: phase.to_string(),
            component: component.map(ToString::to_string),
            downloaded: None,
            total: None,
            rate_bps: None,
            eta_secs: None,
            message: message.to_string(),
        },
    );
}

pub(super) fn emit_init_progress(
    emitter: &EventEmitter,
    task_id: &str,
    component: &str,
    progress: DownloadProgress,
) {
    emit_event(
        emitter,
        BOOTSTRAP_INIT_EVENT,
        BootstrapInitEvent {
            task_id: task_id.to_string(),
            phase: "downloading".to_string(),
            component: Some(component.to_string()),
            downloaded: Some(progress.downloaded),
            total: Some(progress.total),
            rate_bps: Some(progress.rate_bps),
            eta_secs: Some(progress.eta_secs),
            message: String::new(),
        },
    );
}

fn components_all_ready(state: &BootstrapState) -> bool {
    TOOL_COMPONENTS.iter().all(|tool_id| {
        state
            .component(tool_id)
            .is_some_and(|checkpoint| checkpoint.installed && checkpoint.active)
    })
}

fn phase_label(phase: InitPhase) -> &'static str {
    match phase {
        InitPhase::NotStarted => "not_started",
        InitPhase::Resolving => "resolving",
        InitPhase::Downloading => "downloading",
        InitPhase::Verifying => "verifying",
        InitPhase::Staging => "staging",
        InitPhase::Activating => "activating",
        InitPhase::HealthCheck => "health_check",
        InitPhase::Ready => "ready",
        InitPhase::Degraded => "degraded",
        InitPhase::Retryable => "retryable",
        InitPhase::Blocked => "blocked",
    }
}
