//! 单个受管组件的安装管线：resolve → 票据 → 下载 → 校验 → 解压 → 激活 → health check。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use sea_orm::DatabaseConnection;

use super::super::inventory::{self, ORIGIN_MANAGED};
use super::super::types::{DownloadRequest, ResolveToolRequest};
use super::super::{capability, client::AgentPlatformClient};
use super::activation::quarantine_component;
use super::archive::{extract_tool_zip, locate_payload, probe_payload};
use super::download::validate_ticket;
use super::init::{emit_init_event, emit_init_progress};
use super::manifest::{
    marker_matches, push_pending_activation, read_marker, upsert_entry, write_manifest,
    write_marker, InventoryManifest, OwnershipMarker, PendingActivation,
};
use super::preflight::{ensure_disk_headroom, InstallEstimate};
use super::resumable::{download_resumable, DownloadProgress};
use super::runtime::{read_current_pointer, runtime_dir, staging_dir, write_current_pointer};
use super::signature::verify_tool_signature;
use super::state::{BootstrapState, InitPhase};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

/// 单个组件安装结果（IR-005：`deferred` 表示活跃会话存活时延迟激活）。
pub(super) struct ComponentOutcome {
    pub version: String,
    pub deferred: bool,
}

/// 安装并激活单个组件。返回激活的版本。
#[allow(clippy::too_many_arguments)]
pub(super) async fn install_tool_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    manifest: &mut InventoryManifest,
    tool_id: &str,
    channel: &str,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
    active: &BTreeMap<String, String>,
) -> Result<ComponentOutcome, AppCommandError> {
    let current_version = active.get(tool_id).cloned().unwrap_or_default();
    let offer = AgentPlatformClient::resolve_tool(
        conn,
        ResolveToolRequest {
            tool_id,
            current_version: &current_version,
            requested_version: None,
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            // 初始化没有指定版本，按后端契约走正常的推荐版本选择流程。
            reason: "automatic",
        },
    )
    .await?;

    let expected = OwnershipMarker {
        schema: 1,
        component_id: tool_id.to_string(),
        component_kind: "runtime_tool".to_string(),
        version: offer.version.clone(),
        artifact_id: Some(offer.artifact.id.clone()),
        sha256: Some(offer.artifact.sha256.clone()),
        target: capability::current_target().to_string(),
        arch: capability::current_arch().to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        origin: ORIGIN_MANAGED.to_string(),
    };

    let final_dir = runtime_dir(data_dir, tool_id, &offer.version)?;
    let already_installed = read_marker(&final_dir)
        .await
        .is_some_and(|marker| marker_matches(&marker, &expected))
        && active
            .get(tool_id)
            .is_some_and(|version| version == &offer.version);
    if already_installed {
        // 完全匹配 → keep，零下载。
        return Ok(ComponentOutcome {
            version: offer.version,
            deferred: false,
        });
    }
    // IR-005：版本已安装但未激活（活跃会话存活时写入过 pending）→ 零下载，
    // 仅确保 pending 记录存在，激活留给会话结束后的首启消费。
    if defer_while_active
        && read_marker(&final_dir)
            .await
            .is_some_and(|marker| marker_matches(&marker, &expected))
    {
        push_pending_activation(
            data_dir,
            PendingActivation {
                component_id: tool_id.to_string(),
                component_kind: "runtime_tool".to_string(),
                version: offer.version.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
                policy: Some(offer.effective_update_policy.clone()),
                revision: Some(offer.revision),
            },
        )
        .await?;
        tracing::info!(
            tool_id = %tool_id,
            version = %offer.version,
            "[agent-version-center] bootstrap component already installed, activation still deferred"
        );
        return Ok(ComponentOutcome {
            version: offer.version.clone(),
            deferred: true,
        });
    }

    let ticket = AgentPlatformClient::download_tool(
        conn,
        DownloadRequest {
            registry_id: None,
            tool_id: Some(tool_id),
            version_id: &offer.version_id,
            artifact_id: &offer.artifact.id,
            catalog_revision: offer.revision,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
        },
    )
    .await?;
    validate_ticket(
        &offer,
        &ticket.url,
        ticket.size,
        &ticket.sha256,
        &ticket.signature,
    )?;

    // 磁盘预检：归档 + 展开 + staging + 保留旧版本余量。
    emit_init_event(emitter, task_id, "downloading", Some(tool_id), "");
    ensure_disk_headroom(
        data_dir,
        &InstallEstimate {
            archive_bytes: ticket.size.max(0) as u64,
            expanded_bytes: (ticket.size.max(0) as u64).saturating_mul(6),
            retention_bytes: 64 * 1024 * 1024,
        },
    )
    .map_err(|message| AppCommandError::invalid_input(message))?;

    let stage = staging_dir(data_dir, tool_id)?;
    tokio::fs::create_dir_all(&stage)
        .await
        .map_err(AppCommandError::io)?;
    let archive = stage.join("artifact.zip");
    let started = Instant::now();
    let last_emitted = AtomicU64::new(0);
    let progress_cb = move |progress: DownloadProgress| {
        let last = last_emitted.load(Ordering::Relaxed);
        if progress.downloaded.saturating_sub(last) >= 128 * 1024
            || started.elapsed().as_secs() >= 1
        {
            last_emitted.store(progress.downloaded, Ordering::Relaxed);
            emit_init_progress(emitter, task_id, tool_id, progress);
        }
    };
    download_resumable(
        &offer.artifact.id,
        &ticket.url,
        &archive,
        ticket.size,
        &ticket.sha256,
        Some(&progress_cb),
    )
    .await?;

    emit_init_event(emitter, task_id, "verifying", Some(tool_id), "");
    let bytes = tokio::fs::read(&archive)
        .await
        .map_err(AppCommandError::io)?;
    verify_tool_signature(&bytes, &ticket.signature)?;

    emit_init_event(emitter, task_id, "staging", Some(tool_id), "");
    let extract_root = stage.join("payload");
    let tool_id_owned = tool_id.to_string();
    let extract_root_owned = extract_root.clone();
    tokio::task::spawn_blocking(move || {
        extract_tool_zip(&bytes, &extract_root_owned, &tool_id_owned)
    })
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))??;
    let payload = locate_payload(&extract_root, tool_id)?;

    emit_init_event(emitter, task_id, "activating", Some(tool_id), "");
    probe_payload(&payload, tool_id, &offer.version).await?;

    if final_dir.exists() {
        // 同版本目录已存在但 marker 不匹配 → 隔离后重建，绝不直接覆盖。
        quarantine_component(data_dir, &final_dir).await?;
    }
    if let Some(parent) = final_dir.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppCommandError::io)?;
    }
    tokio::fs::rename(&payload, &final_dir)
        .await
        .map_err(AppCommandError::io)?;
    write_marker(&final_dir, &expected).await?;

    inventory::record_tool_ready(
        conn,
        inventory::ReadyToolInstallation {
            tool_id,
            version: &offer.version,
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            origin: ORIGIN_MANAGED,
            artifact_id: Some(&offer.artifact.id),
            expected_sha256: Some(&offer.artifact.sha256),
        },
    )
    .await
    .map_err(inventory_error)?;

    // IR-005：会话存活时不切换该组件版本，写入 pending activations，
    // 待会话结束后的首次启动（bootstrap_initialize）消费并激活。
    if defer_while_active {
        push_pending_activation(
            data_dir,
            PendingActivation {
                component_id: tool_id.to_string(),
                component_kind: "runtime_tool".to_string(),
                version: offer.version.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
                policy: Some(offer.effective_update_policy.clone()),
                revision: Some(offer.revision),
            },
        )
        .await?;
        upsert_entry(
            manifest,
            super::manifest::InventoryEntry {
                component_id: tool_id.to_string(),
                component_kind: "runtime_tool".to_string(),
                version: offer.version.clone(),
                origin: ORIGIN_MANAGED.to_string(),
                artifact_id: Some(offer.artifact.id.clone()),
                sha256: Some(offer.artifact.sha256.clone()),
                path: format!("runtime/{tool_id}"),
                active: false,
            },
        );
        write_manifest(data_dir, manifest).await?;
        tracing::info!(
            tool_id = %tool_id,
            version = %offer.version,
            revision = offer.revision,
            "[agent-version-center] bootstrap component installed, activation deferred (active session)"
        );
        return Ok(ComponentOutcome {
            version: offer.version.clone(),
            deferred: true,
        });
    }

    let previous_pointer = read_current_pointer(data_dir, tool_id).await?;
    write_current_pointer(data_dir, tool_id, &offer.version).await?;
    if let Err(error) = inventory::activate_tool(
        conn,
        tool_id,
        &offer.version,
        &offer.effective_update_policy,
        offer.revision,
    )
    .await
    {
        super::runtime::restore_current_pointer(data_dir, tool_id, previous_pointer).await?;
        quarantine_component(data_dir, &final_dir).await?;
        return Err(inventory_error(error));
    }

    upsert_entry(
        manifest,
        super::manifest::InventoryEntry {
            component_id: tool_id.to_string(),
            component_kind: "runtime_tool".to_string(),
            version: offer.version.clone(),
            origin: ORIGIN_MANAGED.to_string(),
            artifact_id: Some(offer.artifact.id.clone()),
            sha256: Some(offer.artifact.sha256.clone()),
            path: format!("runtime/{tool_id}"),
            active: true,
        },
    );
    write_manifest(data_dir, manifest).await?;

    // health check：从客户端 allowlist 探针验证激活后的版本可执行。
    emit_init_event(emitter, task_id, "health_check", Some(tool_id), "");
    if let Err(error) = probe_payload(&final_dir, tool_id, &offer.version).await {
        // 回滚 LKG 并隔离失败版本，保留诊断。
        super::runtime::restore_current_pointer(data_dir, tool_id, previous_pointer).await?;
        quarantine_component(data_dir, &final_dir).await?;
        return Err(error);
    }
    Ok(ComponentOutcome {
        version: offer.version,
        deferred: false,
    })
}

/// 更新 state 中单个组件的检查点。
pub(super) fn update_checkpoint(state: &mut BootstrapState, tool_id: &str, version: String) {
    let mut checkpoint = state
        .component(tool_id)
        .cloned()
        .unwrap_or_else(|| empty_checkpoint(tool_id));
    checkpoint.version = version;
    checkpoint.installed = true;
    checkpoint.active = true;
    checkpoint.phase = InitPhase::Ready;
    checkpoint.last_error = None;
    state.upsert_component(checkpoint);
}

/// IR-005：组件已安装但激活被延迟（活跃会话存活）时的检查点更新。
pub(super) fn update_checkpoint_deferred(
    state: &mut BootstrapState,
    tool_id: &str,
    version: String,
) {
    let mut checkpoint = state
        .component(tool_id)
        .cloned()
        .unwrap_or_else(|| empty_checkpoint(tool_id));
    checkpoint.version = version;
    checkpoint.installed = true;
    checkpoint.active = false;
    checkpoint.phase = InitPhase::Ready;
    checkpoint.last_error = None;
    state.upsert_component(checkpoint);
}

pub(super) fn empty_checkpoint(component_id: &str) -> super::state::ComponentCheckpoint {
    super::state::ComponentCheckpoint {
        component_id: component_id.to_string(),
        component_kind: "runtime_tool".to_string(),
        version: String::new(),
        phase: InitPhase::NotStarted,
        installed: false,
        active: false,
        last_error: None,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

pub(super) fn inventory_error(error: crate::acp::error::AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
