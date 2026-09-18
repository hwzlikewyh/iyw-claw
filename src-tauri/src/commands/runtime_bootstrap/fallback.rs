mod download;
mod download_support;
mod install;
mod spec;

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::web::event_bridge::EventEmitter;

use super::types::{emit, RuntimeBootstrapEventKind};

pub(crate) struct PinnedRuntimeRequest<'a> {
    pub tool_id: &'a str,
    pub minimum_version: &'a str,
    pub task_id: &'a str,
    pub emitter: &'a EventEmitter,
}

pub(crate) struct PinnedRuntimeArchive {
    pub version: &'static str,
    pub sha256: &'static str,
    pub path: PathBuf,
}

// 仅准备可信归档；写锁、库存记录与激活由调用方统一管理。
pub(crate) async fn download_pinned(
    request: PinnedRuntimeRequest<'_>,
) -> Result<PinnedRuntimeArchive, String> {
    let spec = spec::for_tool(request.tool_id)?;
    if !request.minimum_version.is_empty() {
        let minimum =
            semver::Version::parse(request.minimum_version).map_err(|error| error.to_string())?;
        let pinned = semver::Version::parse(spec.version).map_err(|error| error.to_string())?;
        if pinned < minimum {
            return Err("Pinned runtime is older than the required version".to_string());
        }
    }
    let sha256 = spec
        .expected_sha256
        .ok_or("Pinned runtime digest is missing")?;
    let cache = crate::shared_runtime::root().join("cache/downloads");
    tokio::fs::create_dir_all(&cache)
        .await
        .map_err(|error| error.to_string())?;
    let path = cache.join(&spec.asset);
    download::download_archive(&spec, &path, request.task_id, request.emitter).await?;
    Ok(PinnedRuntimeArchive {
        version: spec.version,
        sha256,
        path,
    })
}

pub(crate) fn availability_failure(error: &crate::app_error::AppCommandError) -> bool {
    use crate::app_error::AppErrorCode;
    cfg!(windows)
        && (error.code == AppErrorCode::NetworkError
            || (error.code == AppErrorCode::InvalidInput
                && matches!(
                    error.detail.as_deref(),
                    Some(
                        "AGENT_TOOL_NOT_FOUND"
                            | "AGENT_TOOL_POLICY_MISSING"
                            | "AGENT_TOOL_VERSION_NOT_FOUND"
                            | "AGENT_TOOL_ARTIFACT_NOT_READY"
                            | "AGENT_STORAGE_UNAVAILABLE"
                            | "AGENT_DOWNLOAD_UNAVAILABLE"
                            | "AGENT_RATE_LIMITED"
                    )
                )))
}

pub(super) struct InstallResult {
    pub(super) version: &'static str,
}

pub(super) async fn install(
    data_dir: &Path,
    tool_id: &str,
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<InstallResult, String> {
    let started = Instant::now();
    let spec = match spec::for_tool(tool_id) {
        Ok(spec) => spec,
        Err(error) => {
            tracing::error!(
                task_id,
                tool_id,
                phase = "spec",
                outcome = "failed",
                duration_ms = started.elapsed().as_millis() as u64,
                "pinned fallback specification unavailable"
            );
            return Err(error);
        }
    };
    tracing::info!(
        task_id,
        tool_id,
        version = spec.version,
        phase = "begin",
        "pinned fallback installation started"
    );
    emit_event(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Log,
        &spec,
        None,
        format!("managed runtime unavailable; using pinned {}", spec.version),
    );
    let result = install::install_component(data_dir, &spec, task_id, emitter).await;
    match result {
        Ok(path) => {
            tracing::info!(
                task_id,
                tool_id,
                version = spec.version,
                outcome = "installed",
                duration_ms = started.elapsed().as_millis() as u64,
                "[runtime-bootstrap] pinned fallback installed"
            );
            emit_event(
                emitter,
                task_id,
                RuntimeBootstrapEventKind::Completed,
                &spec,
                Some(100),
                path.to_string_lossy(),
            );
            Ok(InstallResult {
                version: spec.version,
            })
        }
        Err(error) => {
            tracing::error!(
                task_id,
                tool_id,
                version = spec.version,
                outcome = "failed",
                error_detail_present = true,
                duration_ms = started.elapsed().as_millis() as u64,
                "[runtime-bootstrap] pinned fallback failed"
            );
            emit_event(
                emitter,
                task_id,
                RuntimeBootstrapEventKind::Failed,
                &spec,
                None,
                error.clone(),
            );
            Err(error)
        }
    }
}

fn emit_event(
    emitter: &EventEmitter,
    task_id: &str,
    kind: RuntimeBootstrapEventKind,
    spec: &spec::ComponentSpec,
    percent: Option<u8>,
    payload: impl Into<String>,
) {
    emit(
        emitter,
        task_id,
        kind,
        Some(spec.kind.tool_id().to_string()),
        percent,
        payload,
    );
}
