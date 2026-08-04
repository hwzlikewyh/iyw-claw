mod download;
mod download_support;
mod install;
mod spec;

use std::path::Path;
use std::time::Instant;

use crate::web::event_bridge::EventEmitter;

use super::types::{emit, RuntimeBootstrapEventKind};

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
