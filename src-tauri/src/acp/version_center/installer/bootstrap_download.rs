//! Concurrent-safe resolve, ticket, download, verification, and extraction.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use sea_orm::DatabaseConnection;

use super::archive::{extract_tool_zip, locate_payload, probe_payload};
use super::bootstrap_component::PreparedToolComponent;
use super::download::validate_ticket;
use super::init::{emit_init_event, emit_init_progress};
use super::manifest::OwnershipMarker;
use super::preflight::{ensure_disk_headroom, InstallEstimate};
use super::resumable::{download_resumable, DownloadProgress};
use super::runtime::staging_dir;
use super::signature::verify_tool_signature;
use crate::acp::version_center::capability;
use crate::acp::version_center::client::AgentPlatformClient;
use crate::acp::version_center::types::{DownloadRequest, DownloadTicket, ToolOffer};
use crate::app_error::{AppCommandError, AppErrorCode};
use crate::web::event_bridge::EventEmitter;

const PROGRESS_GRANULARITY_BYTES: u64 = 128 * 1024;
const PROGRESS_INTERVAL_MS: u64 = 1_000;

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare_fresh(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    channel: &str,
    task_id: &str,
    emitter: &EventEmitter,
    current_version: String,
    offer: ToolOffer,
    marker: OwnershipMarker,
    final_dir: PathBuf,
) -> Result<PreparedToolComponent, AppCommandError> {
    let stage = staging_dir(data_dir, tool_id)?;
    let result = prepare_fresh_inner(
        conn,
        data_dir,
        tool_id,
        channel,
        task_id,
        emitter,
        &current_version,
        &offer,
        &stage,
    )
    .await;
    let payload = match result {
        Ok(payload) => payload,
        Err(error) => {
            remove_stage(&stage).await;
            return Err(error);
        }
    };
    Ok(PreparedToolComponent::Fresh {
        offer,
        marker,
        origin: super::super::inventory::ORIGIN_MANAGED,
        stage,
        payload,
        final_dir,
    })
}

#[allow(clippy::too_many_arguments)]
async fn prepare_fresh_inner(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    channel: &str,
    task_id: &str,
    emitter: &EventEmitter,
    current_version: &str,
    offer: &ToolOffer,
    stage: &Path,
) -> Result<PathBuf, AppCommandError> {
    tokio::fs::create_dir_all(stage)
        .await
        .map_err(AppCommandError::io)?;
    let ticket = request_ticket(conn, offer, current_version, channel).await?;
    ensure_disk_space(data_dir, &ticket)?;
    let archive = stage.join("artifact.zip");
    let ticket = download_archive(
        conn,
        offer,
        current_version,
        channel,
        &archive,
        task_id,
        emitter,
        ticket,
    )
    .await?;
    verify_and_extract(&archive, stage, tool_id, offer, &ticket).await
}

fn ensure_disk_space(data_dir: &Path, ticket: &DownloadTicket) -> Result<(), AppCommandError> {
    ensure_disk_headroom(
        data_dir,
        &InstallEstimate {
            archive_bytes: ticket.size.max(0) as u64,
            expanded_bytes: (ticket.size.max(0) as u64).saturating_mul(6),
            retention_bytes: 64 * 1024 * 1024,
        },
    )
    .map_err(AppCommandError::invalid_input)
}

#[allow(clippy::too_many_arguments)]
async fn download_archive(
    conn: &DatabaseConnection,
    offer: &ToolOffer,
    current_version: &str,
    channel: &str,
    archive: &Path,
    task_id: &str,
    emitter: &EventEmitter,
    mut ticket: DownloadTicket,
) -> Result<DownloadTicket, AppCommandError> {
    emit_init_event(emitter, task_id, "downloading", Some(&offer.tool_id), "");
    let progress = progress_callback(emitter, task_id, &offer.tool_id);
    let mut refreshes = 0_u8;
    loop {
        match download_resumable(
            &offer.artifact.id,
            &ticket.url,
            archive,
            ticket.size,
            &ticket.sha256,
            Some(&progress),
        )
        .await
        {
            Ok(()) => return Ok(ticket),
            Err(error) if error.code == AppErrorCode::AuthenticationFailed && refreshes < 2 => {
                refreshes += 1;
                ticket = request_ticket(conn, offer, current_version, channel).await?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn progress_callback<'a>(
    emitter: &'a EventEmitter,
    task_id: &'a str,
    tool_id: &'a str,
) -> impl Fn(DownloadProgress) + Send + Sync + 'a {
    let started = Instant::now();
    let last_emitted = AtomicU64::new(0);
    let last_emitted_at = AtomicU64::new(0);
    move |progress| {
        let last = last_emitted.load(Ordering::Relaxed);
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let last_at = last_emitted_at.load(Ordering::Relaxed);
        if progress.downloaded.saturating_sub(last) >= PROGRESS_GRANULARITY_BYTES
            || elapsed_ms.saturating_sub(last_at) >= PROGRESS_INTERVAL_MS
        {
            last_emitted.store(progress.downloaded, Ordering::Relaxed);
            last_emitted_at.store(elapsed_ms, Ordering::Relaxed);
            emit_init_progress(emitter, task_id, tool_id, progress);
        }
    }
}

async fn request_ticket(
    conn: &DatabaseConnection,
    offer: &ToolOffer,
    current_version: &str,
    channel: &str,
) -> Result<DownloadTicket, AppCommandError> {
    let ticket = AgentPlatformClient::download_tool(
        conn,
        DownloadRequest {
            registry_id: None,
            tool_id: Some(&offer.tool_id),
            version_id: &offer.version_id,
            artifact_id: &offer.artifact.id,
            catalog_revision: offer.revision,
            current_version,
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: capability::RUNTIME,
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
    Ok(ticket)
}

async fn verify_and_extract(
    archive: &Path,
    stage: &Path,
    tool_id: &str,
    offer: &ToolOffer,
    ticket: &DownloadTicket,
) -> Result<PathBuf, AppCommandError> {
    let bytes = tokio::fs::read(archive)
        .await
        .map_err(AppCommandError::io)?;
    verify_tool_signature(&bytes, &ticket.signature)?;
    let extract_root = stage.join("payload");
    let extracted = extract_root.clone();
    let tool = tool_id.to_string();
    tokio::task::spawn_blocking(move || extract_tool_zip(&bytes, &extracted, &tool))
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))??;
    let payload = locate_payload(&extract_root, tool_id)?;
    probe_payload(&payload, tool_id, &offer.version).await?;
    Ok(payload)
}

pub(super) async fn remove_stage(stage: &Path) {
    if let Err(error) = tokio::fs::remove_dir_all(stage).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(stage = %stage.display(), error = %error, "bootstrap staging cleanup failed");
        }
    }
}
