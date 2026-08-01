use std::path::Path;
use std::sync::OnceLock;

use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokio::sync::Mutex;

use super::activation::quarantine_component;
use super::archive::{extract_tool_zip, locate_payload, probe_payload};
use super::download::validate_ticket;
use super::manifest::{
    marker_matches, read_marker, upsert_entry, write_manifest, write_marker, InventoryEntry,
    OwnershipMarker,
};
use super::preflight::{ensure_disk_headroom, InstallEstimate};
use super::resumable::{download_resumable, DownloadProgress};
use super::runtime::{
    read_current_pointer, restore_current_pointer, runtime_dir, staging_dir, write_current_pointer,
};
use super::signature::verify_tool_signature;
use crate::acp::version_center::capability::{self, RUNTIME};
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::inventory::{self, ReadyToolInstallation, ORIGIN_MANAGED};
use crate::acp::version_center::types::{DownloadRequest, ResolveToolRequest, ToolOffer};
use crate::app_error::{AppCommandError, AppErrorCode};

fn install_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedToolInstallResult {
    pub tool_id: String,
    pub version: String,
    pub catalog_revision: u64,
}

/// Install a managed tool, optionally emitting download-progress events.
///
/// `task_id` and `emitter` must both be `Some` or both `None`. When provided,
/// `app://agent-install` events with kind `progress` are emitted during the
/// archive download so the UI can render a progress bar. Downloads are
/// resumable (`.part` + Range) and reuse an already-installed matching version
/// without downloading.
pub async fn install_managed_tool(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    requested_version: Option<&str>,
    channel: &str,
    task_id: Option<&str>,
    emitter: Option<&crate::web::event_bridge::EventEmitter>,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    let _guard = install_lock().lock().await;
    validate_request(tool_id, requested_version, channel)?;
    let settings = inventory::list_tool_settings(conn)
        .await
        .map_err(inventory_error)?;
    let setting = settings.iter().find(|item| item.tool_id == tool_id);
    let current_version = setting
        .and_then(|item| item.active_version.as_deref())
        .unwrap_or_default();
    let pinned_version = setting.and_then(|item| item.pinned_version.as_deref());
    let offer = AgentPlatformClient::resolve_tool(
        conn,
        ResolveToolRequest {
            tool_id,
            current_version,
            requested_version,
            pinned_version,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason: "manual",
        },
    )
    .await?;
    install_offer(conn, data_dir, &offer, channel, task_id, emitter).await
}

async fn install_offer(
    conn: &DatabaseConnection,
    data_dir: &Path,
    offer: &ToolOffer,
    channel: &str,
    task_id: Option<&str>,
    emitter: Option<&crate::web::event_bridge::EventEmitter>,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    let stage = staging_dir(data_dir, &offer.tool_id)?;
    let result = install_offer_inner(conn, data_dir, &stage, offer, channel, task_id, emitter).await;
    if let Err(error) = tokio::fs::remove_dir_all(&stage).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                tool_id = %offer.tool_id,
                version = %offer.version,
                error = %error,
                "[agent-version-center] managed tool staging cleanup failed"
            );
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn install_offer_inner(
    conn: &DatabaseConnection,
    data_dir: &Path,
    stage: &Path,
    offer: &ToolOffer,
    channel: &str,
    task_id: Option<&str>,
    emitter: Option<&crate::web::event_bridge::EventEmitter>,
) -> Result<ManagedToolInstallResult, AppCommandError> {
    tokio::fs::create_dir_all(stage)
        .await
        .map_err(AppCommandError::io)?;
    let final_dir = runtime_dir(data_dir, &offer.tool_id, &offer.version)?;
    let expected_marker = managed_marker(offer);

    // keep 快速路径：同版本已安装且 marker 完全匹配 + active pointer 一致 → 零下载。
    let pointer_version = read_current_pointer(data_dir, &offer.tool_id)
        .await?
        .and_then(|bytes| {
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| value.get("version")?.as_str().map(ToString::to_string))
        });
    if read_marker(&final_dir)
        .await
        .is_some_and(|marker| marker_matches(&marker, &expected_marker))
        && pointer_version.as_deref() == Some(offer.version.as_str())
    {
        tracing::info!(
            tool_id = %offer.tool_id,
            version = %offer.version,
            "[agent-version-center] managed tool already installed, keeping"
        );
        return Ok(ManagedToolInstallResult {
            tool_id: offer.tool_id.clone(),
            version: offer.version.clone(),
            catalog_revision: offer.revision,
        });
    }

    let ticket = AgentPlatformClient::download_tool(
        conn,
        DownloadRequest {
            registry_id: None,
            tool_id: Some(&offer.tool_id),
            version_id: &offer.version_id,
            artifact_id: &offer.artifact.id,
            catalog_revision: offer.revision,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
        },
    )
    .await?;
    validate_ticket(
        offer,
        &ticket.url,
        ticket.size,
        &ticket.sha256,
        &ticket.signature,
    )?;
    let archive = stage.join("artifact.zip");

    // 磁盘预检：归档 + 展开 + staging + 保留旧版本余量。
    ensure_disk_headroom(
        data_dir,
        &InstallEstimate {
            archive_bytes: ticket.size.max(0) as u64,
            expanded_bytes: (ticket.size.max(0) as u64).saturating_mul(6),
            retention_bytes: 64 * 1024 * 1024,
        },
    )
    .map_err(|message| AppCommandError::invalid_input(message))?;

    // 进度回调：app://agent-install 百分比事件。
    let progress_cb: Option<Box<dyn Fn(DownloadProgress) + Send + Sync>> =
        match (task_id, emitter) {
            (Some(tid), Some(em)) => {
                let tid = tid.to_string();
                let em = em.clone();
                Some(Box::new(move |progress: DownloadProgress| {
                    let pct = if progress.total > 0 {
                        ((progress.downloaded as f64 / progress.total as f64) * 100.0) as u8
                    } else {
                        0
                    };
                    crate::commands::acp::emit_managed_tool_progress(&em, &tid, pct);
                }))
            }
            _ => None,
        };

    // 可续传下载；票据过期（401/403）刷新票据重试，最多 2 次。
    let mut current_url = ticket.url.clone();
    let mut refresh_attempts = 0_u32;
    loop {
        match download_resumable(
            &current_url,
            &archive,
            ticket.size,
            &ticket.sha256,
            progress_cb.as_deref(),
        )
        .await
        {
            Ok(()) => break,
            Err(error)
                if error.code == AppErrorCode::AuthenticationFailed && refresh_attempts < 2 =>
            {
                refresh_attempts += 1;
                let refreshed = AgentPlatformClient::download_tool(
                    conn,
                    DownloadRequest {
                        registry_id: None,
                        tool_id: Some(&offer.tool_id),
                        version_id: &offer.version_id,
                        artifact_id: &offer.artifact.id,
                        catalog_revision: offer.revision,
                        client_version: env!("CARGO_PKG_VERSION"),
                        runtime: RUNTIME,
                        target: capability::current_target(),
                        arch: capability::current_arch(),
                        channel,
                    },
                )
                .await?;
                validate_ticket(
                    offer,
                    &refreshed.url,
                    refreshed.size,
                    &refreshed.sha256,
                    &refreshed.signature,
                )?;
                current_url = refreshed.url.clone();
            }
            Err(error) => return Err(error),
        }
    }

    let bytes = tokio::fs::read(&archive)
        .await
        .map_err(AppCommandError::io)?;
    verify_tool_signature(&bytes, &ticket.signature)?;

    let extract_root = stage.join("payload");
    let tool_id = offer.tool_id.clone();
    let extracted = extract_root.clone();
    tokio::task::spawn_blocking(move || extract_tool_zip(&bytes, &extracted, &tool_id))
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))??;
    let payload = locate_payload(&extract_root, &offer.tool_id)?;
    probe_payload(&payload, &offer.tool_id, &offer.version).await?;
    let confirmed = confirm_offer(conn, offer, channel).await?;

    if final_dir.exists() {
        // 同版本目录已存在但 marker 不匹配 → 隔离后重建，绝不直接覆盖。
        quarantine_component(data_dir, &final_dir).await?;
    }
    let parent = final_dir.parent().ok_or_else(|| {
        AppCommandError::configuration_invalid("Managed tool runtime path is invalid")
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&payload, &final_dir)
        .await
        .map_err(AppCommandError::io)?;
    write_marker(&final_dir, &expected_marker).await?;

    inventory::record_tool_ready(
        conn,
        ReadyToolInstallation {
            tool_id: &offer.tool_id,
            version: &offer.version,
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            origin: ORIGIN_MANAGED,
            artifact_id: Some(&offer.artifact.id),
            expected_sha256: Some(&offer.artifact.sha256),
        },
    )
    .await
    .map_err(inventory_error)?;

    let previous_pointer = read_current_pointer(data_dir, &offer.tool_id).await?;
    write_current_pointer(data_dir, &offer.tool_id, &offer.version).await?;
    if let Err(error) = inventory::activate_tool(
        conn,
        &offer.tool_id,
        &offer.version,
        &confirmed.effective_update_policy,
        confirmed.revision,
    )
    .await
    {
        restore_current_pointer(data_dir, &offer.tool_id, previous_pointer).await?;
        quarantine_component(data_dir, &final_dir).await?;
        return Err(inventory_error(error));
    }

    {
        let mut manifest = super::manifest::read_manifest(data_dir).await?;
        upsert_entry(
            &mut manifest,
            InventoryEntry {
                component_id: offer.tool_id.clone(),
                component_kind: "runtime_tool".to_string(),
                version: offer.version.clone(),
                origin: ORIGIN_MANAGED.to_string(),
                artifact_id: Some(offer.artifact.id.clone()),
                sha256: Some(offer.artifact.sha256.clone()),
                path: format!("runtime/{}", offer.tool_id),
                active: true,
            },
        );
        write_manifest(data_dir, &manifest).await?;
    }
    // health check：从客户端 allowlist 探针验证激活后的版本可执行。
    if let Err(error) = probe_payload(&final_dir, &offer.tool_id, &offer.version).await {
        // 回滚 LKG 并隔离失败版本，保留诊断。
        restore_current_pointer(data_dir, &offer.tool_id, previous_pointer).await?;
        quarantine_component(data_dir, &final_dir).await?;
        return Err(error);
    }
    tracing::info!(
        tool_id = %offer.tool_id,
        version = %offer.version,
        revision = confirmed.revision,
        "[agent-version-center] managed tool installed and activated"
    );
    Ok(ManagedToolInstallResult {
        tool_id: offer.tool_id.clone(),
        version: offer.version.clone(),
        catalog_revision: confirmed.revision,
    })
}

fn managed_marker(offer: &ToolOffer) -> OwnershipMarker {
    OwnershipMarker {
        schema: 1,
        component_id: offer.tool_id.clone(),
        component_kind: "runtime_tool".to_string(),
        version: offer.version.clone(),
        artifact_id: Some(offer.artifact.id.clone()),
        sha256: Some(offer.artifact.sha256.clone()),
        target: capability::current_target().to_string(),
        arch: capability::current_arch().to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        origin: ORIGIN_MANAGED.to_string(),
    }
}

async fn confirm_offer(
    conn: &DatabaseConnection,
    offer: &ToolOffer,
    channel: &str,
) -> Result<ToolOffer, AppCommandError> {
    let confirmed = AgentPlatformClient::resolve_tool(
        conn,
        ResolveToolRequest {
            tool_id: &offer.tool_id,
            current_version: "",
            requested_version: Some(&offer.version),
            pinned_version: None,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel,
            reason: "recovery",
        },
    )
    .await?;
    if confirmed.version_id != offer.version_id
        || confirmed.version != offer.version
        || confirmed.artifact.id != offer.artifact.id
        || !confirmed
            .artifact
            .sha256
            .eq_ignore_ascii_case(&offer.artifact.sha256)
    {
        return Err(AppCommandError::invalid_input(
            "Managed tool offer changed before activation",
        ));
    }
    Ok(confirmed)
}

fn validate_request(
    tool_id: &str,
    requested_version: Option<&str>,
    channel: &str,
) -> Result<(), AppCommandError> {
    if !capability::known_tool(tool_id) || !matches!(channel, "stable" | "beta") {
        return Err(AppCommandError::invalid_input(
            "Invalid managed tool request",
        ));
    }
    if let Some(version) = requested_version.filter(|value| !value.trim().is_empty()) {
        semver::Version::parse(version.trim())
            .map_err(|_| AppCommandError::invalid_input("Invalid managed tool version"))?;
    }
    Ok(())
}

fn inventory_error(error: crate::acp::error::AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed(error.to_string())
}
