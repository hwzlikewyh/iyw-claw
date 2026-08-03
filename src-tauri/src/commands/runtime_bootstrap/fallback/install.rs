use std::path::{Path, PathBuf};

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
    let final_dir =
        runtime_dir(data_dir, spec.kind.tool_id(), spec.version).map_err(command_error)?;
    if reuse_existing(data_dir, &final_dir, spec).await? {
        return Ok(final_dir);
    }
    let staging = staging_dir(data_dir, spec);
    let result = install_inner(data_dir, &staging, &final_dir, spec, task_id, emitter).await;
    if let Err(error) = tokio::fs::remove_dir_all(&staging).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                tool_id = spec.kind.tool_id(),
                path = %staging.display(),
                error = %error,
                "[runtime-bootstrap] fallback staging cleanup failed"
            );
        }
    }
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
    download_archive(spec, &archive, task_id, emitter).await?;
    emit_event(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Log,
        spec,
        None,
        format!("extracting {}", spec.asset),
    );
    let payload = extract_payload(&archive, staging, spec).await?;
    activate_payload(data_dir, &payload, final_dir, spec).await?;
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
