use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::acp::version_center::{
    extract_tool_zip, locate_payload, runtime_dir, write_current_pointer,
};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

use super::download::download_archive;
use super::spec::ComponentSpec;
use super::{emit_event, RuntimeBootstrapEventKind};

pub(super) async fn install_component(
    data_dir: &Path,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<PathBuf, String> {
    let started = Instant::now();
    tracing::info!(
        task_id,
        tool_id = spec.kind.tool_id(),
        version = spec.version,
        phase = "begin",
        "fallback component installation started"
    );
    let final_dir = match runtime_dir(data_dir, spec.kind.tool_id(), spec.version) {
        Ok(final_dir) => final_dir,
        Err(error) => {
            tracing::error!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "runtime_dir",
                duration_ms = started.elapsed().as_millis() as u64,
                "fallback runtime destination rejected"
            );
            return Err(command_error(error));
        }
    };
    match reuse_existing(data_dir, &final_dir, spec).await {
        Ok(true) => {
            tracing::info!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                outcome = "reused",
                duration_ms = started.elapsed().as_millis() as u64,
                "fallback component already installed"
            );
            return Ok(final_dir);
        }
        Ok(false) => {}
        Err(error) => {
            tracing::error!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "reuse",
                error_detail_present = true,
                duration_ms = started.elapsed().as_millis() as u64,
                "fallback component reuse failed"
            );
            return Err(error);
        }
    }
    let staging = staging_dir(data_dir, spec);
    let result = install_inner(data_dir, &staging, &final_dir, spec, task_id, emitter).await;
    if let Err(error) = tokio::fs::remove_dir_all(&staging).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "cleanup",
                error_kind = ?error.kind(),
                "[runtime-bootstrap] fallback staging cleanup failed"
            );
        }
    }
    tracing::info!(
        task_id,
        tool_id = spec.kind.tool_id(),
        version = spec.version,
        outcome = if result.is_ok() {
            "installed"
        } else {
            "failed"
        },
        duration_ms = started.elapsed().as_millis() as u64,
        "fallback component installation finished"
    );
    result
}

async fn install_inner(
    data_dir: &Path,
    staging: &Path,
    final_dir: &Path,
    spec: &ComponentSpec,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<PathBuf, String> {
    let downloads = data_dir.join("runtime").join("downloads");
    tokio::fs::create_dir_all(&downloads)
        .await
        .map_err(|error| format!("failed to create {}: {error}", downloads.display()))?;
    let archive = downloads.join(&spec.asset);
    let download_started = Instant::now();
    match download_archive(spec, &archive, task_id, emitter).await {
        Ok(()) => tracing::info!(
            task_id,
            tool_id = spec.kind.tool_id(),
            version = spec.version,
            phase = "download",
            outcome = "completed",
            duration_ms = download_started.elapsed().as_millis() as u64,
            "fallback download stage finished"
        ),
        Err(error) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "download",
                outcome = "failed",
                error_detail_present = true,
                duration_ms = download_started.elapsed().as_millis() as u64,
                "fallback download stage failed"
            );
            return Err(error);
        }
    }
    emit_event(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Log,
        spec,
        None,
        format!("extracting {}", spec.asset),
    );
    let extract_started = Instant::now();
    let payload = match extract_payload(&archive, staging, spec).await {
        Ok(payload) => {
            tracing::info!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "extract",
                outcome = "completed",
                duration_ms = extract_started.elapsed().as_millis() as u64,
                "fallback extraction finished"
            );
            payload
        }
        Err(error) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "extract",
                outcome = "failed",
                error_detail_present = true,
                duration_ms = extract_started.elapsed().as_millis() as u64,
                "fallback extraction failed"
            );
            return Err(error);
        }
    };
    let activate_started = Instant::now();
    match activate_payload(data_dir, &payload, final_dir, spec).await {
        Ok(()) => tracing::info!(
            task_id,
            tool_id = spec.kind.tool_id(),
            version = spec.version,
            phase = "activate",
            outcome = "completed",
            duration_ms = activate_started.elapsed().as_millis() as u64,
            "fallback activation finished"
        ),
        Err(error) => {
            tracing::warn!(
                task_id,
                tool_id = spec.kind.tool_id(),
                version = spec.version,
                phase = "activate",
                outcome = "failed",
                error_detail_present = true,
                duration_ms = activate_started.elapsed().as_millis() as u64,
                "fallback activation failed"
            );
            return Err(error);
        }
    }
    Ok(final_dir.to_path_buf())
}

async fn reuse_existing(
    data_dir: &Path,
    final_dir: &Path,
    spec: &ComponentSpec,
) -> Result<bool, String> {
    if !final_dir.is_dir() || locate_payload(final_dir, spec.kind.tool_id()).is_err() {
        return Ok(false);
    }
    write_current_pointer(data_dir, spec.kind.tool_id(), spec.version)
        .await
        .map_err(command_error)?;
    Ok(true)
}

async fn extract_payload(
    archive: &Path,
    staging: &Path,
    spec: &ComponentSpec,
) -> Result<PathBuf, String> {
    let bytes = tokio::fs::read(archive)
        .await
        .map_err(|error| format!("failed to read {}: {error}", archive.display()))?;
    let root = staging.to_path_buf();
    let tool_id = spec.kind.tool_id().to_string();
    tokio::task::spawn_blocking(move || extract_tool_zip(&bytes, &root, &tool_id))
        .await
        .map_err(|error| format!("fallback extraction task failed: {error}"))?
        .map_err(command_error)?;
    locate_payload(staging, spec.kind.tool_id()).map_err(command_error)
}

async fn activate_payload(
    data_dir: &Path,
    payload: &Path,
    final_dir: &Path,
    spec: &ComponentSpec,
) -> Result<(), String> {
    let component_root = data_dir.join("runtime").join(spec.kind.tool_id());
    if !final_dir.starts_with(&component_root) {
        return Err("fallback activation path escaped the managed runtime root".to_string());
    }
    if final_dir.exists() {
        tokio::fs::remove_dir_all(final_dir)
            .await
            .map_err(|error| format!("failed to replace {}: {error}", final_dir.display()))?;
    }
    let parent = final_dir
        .parent()
        .ok_or_else(|| "fallback runtime target has no parent".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    tokio::fs::rename(payload, final_dir)
        .await
        .map_err(|error| format!("failed to activate fallback runtime: {error}"))?;
    write_current_pointer(data_dir, spec.kind.tool_id(), spec.version)
        .await
        .map_err(command_error)
}

fn staging_dir(data_dir: &Path, spec: &ComponentSpec) -> PathBuf {
    data_dir
        .join("runtime")
        .join(spec.kind.tool_id())
        .join(".fallback-staging")
        .join(uuid::Uuid::new_v4().to_string())
}

fn command_error(error: AppCommandError) -> String {
    match error.detail {
        Some(detail) => format!("{}: {detail}", error.message),
        None => error.message,
    }
}
