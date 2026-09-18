use sea_orm::DatabaseConnection;
use std::path::{Path, PathBuf};

use super::archive::{extract_tool_zip, locate_payload, probe_payload};
use super::bootstrap_component::PreparedToolComponent;
use super::bootstrap_download::remove_stage;
use super::init::emit_init_event;
use super::manifest::OwnershipMarker;
use super::preflight::{ensure_disk_headroom, InstallEstimate};
use super::runtime::{runtime_dir, staging_dir};
use crate::acp::version_center::types::{ToolArtifact, ToolOffer};
use crate::acp::version_center::{capability, inventory::ORIGIN_PINNED};
use crate::app_error::AppCommandError;
use crate::commands::runtime_bootstrap::fallback::{
    self, PinnedRuntimeArchive, PinnedRuntimeRequest,
};
use crate::web::event_bridge::EventEmitter;

const EXPANDED_SIZE_FACTOR: u64 = 6;
const RETENTION_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct FallbackRequest<'a> {
    pub conn: &'a DatabaseConnection,
    pub channel: &'a str,
    pub data_dir: &'a Path,
    pub tool_id: &'a str,
    pub minimum_version: &'a str,
    pub task_id: &'a str,
    pub emitter: &'a EventEmitter,
}

pub(super) async fn prepare(
    request: FallbackRequest<'_>,
    original: AppCommandError,
) -> Result<PreparedToolComponent, AppCommandError> {
    if !fallback::availability_failure(&original) {
        return Err(original);
    }
    tracing::warn!(tool_id = request.tool_id, task_id = request.task_id,
        error_code = ?original.code, cause = %super::runtime_seed::error_summary(&original),
        "[runtime-bootstrap] preparing pinned fallback after delivery failure");
    emit_init_event(
        request.emitter,
        request.task_id,
        "downloading",
        Some(request.tool_id),
        "安装源暂不可用，正在准备固定版本备用包并校验完整性",
    );
    prepare_archive(&request).await.map_err(|error| {
        let cause = super::runtime_seed::error_summary(&original);
        let fallback = super::runtime_seed::error_summary(&error);
        tracing::warn!(
            tool_id = request.tool_id,
            cause,
            fallback,
            "[runtime-bootstrap] pinned fallback failed"
        );
        error.with_detail(format!("primary={cause}; fallback={fallback}"))
    })
}

async fn prepare_archive(
    request: &FallbackRequest<'_>,
) -> Result<PreparedToolComponent, AppCommandError> {
    let minimum_version = minimum_version(request).await?;
    let archive = fallback::download_pinned(PinnedRuntimeRequest {
        tool_id: request.tool_id,
        minimum_version: &minimum_version,
        task_id: request.task_id,
        emitter: request.emitter,
    })
    .await
    .map_err(AppCommandError::task_execution_failed)?;
    let stage = staging_dir(request.data_dir, request.tool_id)?;
    emit_init_event(
        request.emitter,
        request.task_id,
        "staging",
        Some(request.tool_id),
        "",
    );
    let result = stage_archive(request, &archive, &stage).await;
    if result.is_err() {
        remove_stage(&stage).await;
    }
    result
}

async fn minimum_version(request: &FallbackRequest<'_>) -> Result<String, AppCommandError> {
    let catalog = crate::acp::version_center::catalog::CatalogStore::load(request.conn).await;
    let snapshot = catalog.view().await.snapshot;
    let Some(tool) = snapshot
        .tools
        .iter()
        .find(|tool| tool.tool_id == request.tool_id && tool.channel == request.channel)
    else {
        return Ok(request.minimum_version.to_string());
    };
    if tool.status == "disabled" {
        return Err(AppCommandError::invalid_input("Managed tool is disabled")
            .with_detail("AGENT_TOOL_DISABLED"));
    }
    let mut minimum = None;
    for value in [request.minimum_version, tool.minimum_safe_version.as_str()] {
        if value.is_empty() {
            continue;
        }
        let version = semver::Version::parse(value)
            .map_err(|_| AppCommandError::invalid_input("Runtime minimum version is invalid"))?;
        minimum = Some(minimum.map_or(version.clone(), |current: semver::Version| {
            current.max(version)
        }));
    }
    Ok(minimum.map(|value| value.to_string()).unwrap_or_default())
}

async fn stage_archive(
    request: &FallbackRequest<'_>,
    archive: &PinnedRuntimeArchive,
    stage: &Path,
) -> Result<PreparedToolComponent, AppCommandError> {
    let size = tokio::fs::metadata(&archive.path)
        .await
        .map_err(AppCommandError::io)?
        .len();
    ensure_disk_headroom(
        archive.path.parent().unwrap_or(stage),
        &InstallEstimate {
            archive_bytes: 0,
            expanded_bytes: size.saturating_mul(EXPANDED_SIZE_FACTOR),
            retention_bytes: RETENTION_BYTES,
        },
    )
    .map_err(AppCommandError::invalid_input)?;
    let payload = extract_archive(archive, stage, request.tool_id).await?;
    let offer = offer(request.tool_id, archive, size)?;
    let marker = OwnershipMarker {
        schema: 1,
        component_id: request.tool_id.to_string(),
        component_kind: "runtime_tool".to_string(),
        version: offer.version.clone(),
        artifact_id: Some(offer.artifact.id.clone()),
        sha256: Some(archive.sha256.to_string()),
        target: capability::current_target().to_string(),
        arch: capability::current_arch().to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        origin: ORIGIN_PINNED.to_string(),
    };
    Ok(PreparedToolComponent::Fresh {
        final_dir: runtime_dir(request.data_dir, request.tool_id, archive.version)?,
        offer,
        marker,
        origin: ORIGIN_PINNED,
        stage: stage.to_path_buf(),
        payload,
    })
}

async fn extract_archive(
    archive: &PinnedRuntimeArchive,
    stage: &Path,
    tool_id: &str,
) -> Result<PathBuf, AppCommandError> {
    let bytes = tokio::fs::read(&archive.path)
        .await
        .map_err(AppCommandError::io)?;
    let extracted = stage.join("payload");
    let destination = extracted.clone();
    let tool = tool_id.to_string();
    tokio::task::spawn_blocking(move || extract_tool_zip(&bytes, &destination, &tool))
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))??;
    let payload = locate_payload(&extracted, tool_id)?;
    probe_payload(&payload, tool_id, archive.version).await?;
    Ok(payload)
}

fn offer(
    tool_id: &str,
    archive: &PinnedRuntimeArchive,
    size: u64,
) -> Result<ToolOffer, AppCommandError> {
    let artifact_id = format!("pinned-runtime:{tool_id}:{}", archive.sha256);
    Ok(ToolOffer {
        revision: 0,
        tool_id: tool_id.to_string(),
        version_id: artifact_id.clone(),
        version: archive.version.to_string(),
        channel: "stable".to_string(),
        security_status: "verified".to_string(),
        selection_reason: ORIGIN_PINNED.to_string(),
        effective_update_policy: "recommended".to_string(),
        required: true,
        artifact: ToolArtifact {
            id: artifact_id,
            runtime: capability::RUNTIME.to_string(),
            target: capability::current_target().to_string(),
            arch: capability::current_arch().to_string(),
            package_kind: "zip".to_string(),
            size: i64::try_from(size)
                .map_err(|_| AppCommandError::invalid_input("Pinned archive is too large"))?,
            sha256: archive.sha256.to_string(),
        },
    })
}
