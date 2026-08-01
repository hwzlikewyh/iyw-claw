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

use super::component::{install_tool_component, update_checkpoint, update_checkpoint_deferred};
use super::manifest::{
    active_versions, digest_managed_root, pending_activations_path, read_manifest,
    read_pending_activations, upsert_entry, write_manifest, write_pending_activations,
    InventoryEntry, InventoryManifest, PendingActivation,
};
use super::migration::{migration_receipt, run_legacy_migration};
use super::resumable::DownloadProgress;
use super::state::{
    acquire_writer_lock, read_state, write_state, BootstrapState, InitPhase,
};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;
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
    let migrated = run_legacy_migration(data_dir).await?.receipt_written;

    // IR-005：会话结束后的首次启动消费 pending activations（无活跃会话时）。
    // 消费先于 resolve：激活后的版本进入 active map，后续 resolve 命中 keep，
    // 避免已安装版本被重复下载。
    if !defer_while_active {
        consume_pending_activations(conn, data_dir).await?;
    }

    let mut state = read_state(data_dir).await?;
    if state.phase == InitPhase::Ready && components_all_ready(&state) {
        return bootstrap_init_status(data_dir).await;
    }

    state.set_phase(InitPhase::Resolving);
    write_state(data_dir, &state).await?;
    emit_init_event(emitter, task_id, "resolving", None, "");

    let mut manifest = read_manifest(data_dir).await?;
    let active = active_versions(&manifest);

    let mut deferred_components: Vec<&str> = Vec::new();
    for tool_id in TOOL_COMPONENTS {
        emit_init_event(emitter, task_id, "resolving", Some(tool_id), "");
        match install_tool_component(
            conn,
            data_dir,
            &mut manifest,
            tool_id,
            channel,
            defer_while_active,
            task_id,
            emitter,
            &mut state,
            &active,
        )
        .await
        {
            Ok(outcome) => {
                if outcome.deferred {
                    deferred_components.push(tool_id);
                    update_checkpoint_deferred(&mut state, tool_id, outcome.version);
                } else {
                    update_checkpoint(&mut state, tool_id, outcome.version);
                }
                write_state(data_dir, &state).await?;
            }
            Err(error) => {
                let healthy = has_healthy_inventory(&manifest);
                state.set_phase(if healthy {
                    InitPhase::Degraded
                } else {
                    InitPhase::Blocked
                });
                if let Some(checkpoint) = state
                    .components
                    .iter_mut()
                    .find(|item| item.component_id == tool_id)
                {
                    checkpoint.last_error = Some(error.message.clone());
                    checkpoint.phase = state.phase;
                }
                write_state(data_dir, &state).await?;
                emit_init_event(
                    emitter,
                    task_id,
                    phase_label(state.phase),
                    Some(tool_id),
                    &error.message,
                );
                return bootstrap_init_status(data_dir).await;
            }
        }
    }

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

/// IR-005：消费待激活记录（会话结束后的首次启动调用）。
///
/// 逐条尝试激活；成功项移除，失败项保留并告警（下次启动重试）。
async fn consume_pending_activations(
    conn: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(), AppCommandError> {
    let pending = read_pending_activations(data_dir).await?;
    if pending.is_empty() {
        return Ok(());
    }
    let mut remaining = Vec::new();
    let mut changed = false;
    for item in pending {
        match activate_pending_component(conn, data_dir, &item).await {
            Ok(()) => {
                changed = true;
                tracing::info!(
                    component_id = %item.component_id,
                    version = %item.version,
                    "[agent-version-center] pending activation consumed"
                );
            }
            Err(error) => {
                tracing::warn!(
                    component_id = %item.component_id,
                    version = %item.version,
                    error = %error,
                    "[agent-version-center] pending activation kept for next startup"
                );
                remaining.push(item);
            }
        }
    }
    if changed {
        write_pending_activations(data_dir, &remaining).await?;
    }
    Ok(())
}

/// 激活单条待激活记录（按 `component_kind` 分派）。
async fn activate_pending_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    pending: &PendingActivation,
) -> Result<(), AppCommandError> {
    let policy = pending.policy.as_deref().unwrap_or("recommended");
    let revision = pending.revision.unwrap_or(0);
    match pending.component_kind.as_str() {
        "runtime_tool" => {
            if !super::super::capability::known_tool(&pending.component_id) {
                return Err(AppCommandError::invalid_input(format!(
                    "Unknown managed tool in pending activation: {}",
                    pending.component_id
                )));
            }
            super::runtime::write_current_pointer(
                data_dir,
                &pending.component_id,
                &pending.version,
            )
            .await?;
            super::super::inventory::activate_tool(
                conn,
                &pending.component_id,
                &pending.version,
                policy,
                revision,
            )
            .await
            .map_err(pending_inventory_error)?;
            mark_manifest_active(data_dir, &pending.component_id, &pending.version, "runtime")
                .await?;
            Ok(())
        }
        "agent" => {
            let agent_type: AgentType = serde_json::from_str(&pending.component_id).map_err(
                |error| {
                    AppCommandError::configuration_invalid(format!(
                        "Pending agent activation has invalid agent type: {error}"
                    ))
                },
            )?;
            super::super::inventory::activate_agent(
                conn,
                agent_type,
                &pending.version,
                policy,
                revision,
            )
            .await
            .map_err(pending_inventory_error)?;
            mark_manifest_active(data_dir, &pending.component_id, &pending.version, "agents")
                .await?;
            Ok(())
        }
        kind => Err(AppCommandError::invalid_input(format!(
            "Unsupported pending activation component kind: {kind}"
        ))),
    }
}

/// 将 manifest 中已存在条目翻转为 active（保留 artifact 元数据）；
/// 不存在时补一条最小条目。
async fn mark_manifest_active(
    data_dir: &Path,
    component_id: &str,
    version: &str,
    directory: &str,
) -> Result<(), AppCommandError> {
    let mut manifest = read_manifest(data_dir).await?;
    let path = format!("{directory}/{component_id}");
    let existing = manifest
        .entries
        .iter_mut()
        .find(|item| item.component_id == component_id && item.version == version);
    match existing {
        Some(item) => {
            item.active = true;
            item.path = path;
        }
        None => {
            upsert_entry(
                &mut manifest,
                InventoryEntry {
                    component_id: component_id.to_string(),
                    component_kind: if directory == "agents" {
                        "agent".to_string()
                    } else {
                        "runtime_tool".to_string()
                    },
                    version: version.to_string(),
                    origin: "managed".to_string(),
                    artifact_id: None,
                    sha256: None,
                    path,
                    active: true,
                },
            );
        }
    }
    write_manifest(data_dir, &manifest).await
}

fn pending_inventory_error(error: crate::acp::error::AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
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

fn has_healthy_inventory(_manifest: &InventoryManifest) -> bool {
    !_manifest.entries.is_empty()
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

/// 待激活文件路径导出（供活跃会话场景记录 pending activation）。
pub fn pending_activations_file(data_dir: &Path) -> std::path::PathBuf {
    pending_activations_path(data_dir)
}
