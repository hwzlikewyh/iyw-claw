mod download;
mod install;
mod spec;

use std::path::Path;

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
    let spec = spec::for_tool(tool_id)?;
    emit_event(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Log,
        &spec,
        None,
        format!("managed catalog empty; using pinned {}", spec.version),
    );
    let result = install::install_component(data_dir, &spec, task_id, emitter).await;
    match result {
        Ok(path) => {
            tracing::info!(
                tool_id,
                version = spec.version,
                path = %path.display(),
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
                tool_id,
                version = spec.version,
                error,
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
