use std::path::Path;

use sea_orm::DatabaseConnection;

use crate::acp::version_center::{install_managed_tool, managed_tool_executable};
use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

use super::fallback;
use super::types::{
    emit, RuntimeBootstrapEventKind, RuntimeComponentReport, RuntimeComponentStatus,
};

pub(super) async fn ensure_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    channel: &str,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
) -> RuntimeComponentReport {
    if let Some(report) = ready_component(tool_id) {
        return report;
    }
    match install_managed_tool(
        conn,
        data_dir,
        tool_id,
        None,
        channel,
        defer_while_active,
        Some(task_id),
        Some(emitter),
    )
    .await
    {
        Ok(result) => installed_report(tool_id, &result.version, task_id, emitter),
        Err(error) if allows_fallback(&error) => {
            install_fallback(data_dir, tool_id, task_id, emitter, &error).await
        }
        Err(error) => failed_managed_report(tool_id, error),
    }
}

fn ready_component(tool_id: &str) -> Option<RuntimeComponentReport> {
    managed_tool_executable(tool_id)
        .or_else(|| which::which(tool_id).ok())
        .map(|path| RuntimeComponentReport {
            status: RuntimeComponentStatus::Ready,
            detail: Some(path.to_string_lossy().into_owned()),
        })
}

fn installed_report(
    tool_id: &str,
    version: &str,
    task_id: &str,
    emitter: &EventEmitter,
) -> RuntimeComponentReport {
    emit(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Completed,
        Some(tool_id.to_string()),
        Some(100),
        format!("{tool_id} {version} installed"),
    );
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Installed,
        detail: Some(format!("{tool_id} {version}")),
    }
}

fn allows_fallback(error: &AppCommandError) -> bool {
    matches!(
        error.detail.as_deref(),
        Some(
            "AGENT_TOOL_NOT_FOUND"
                | "AGENT_TOOL_VERSION_NOT_FOUND"
                | "AGENT_TOOL_ARTIFACT_NOT_READY"
                | "AGENT_VERSION_NOT_PUBLISHED"
        )
    )
}

async fn install_fallback(
    data_dir: &Path,
    tool_id: &str,
    task_id: &str,
    emitter: &EventEmitter,
    managed_error: &AppCommandError,
) -> RuntimeComponentReport {
    tracing::warn!(
        tool_id,
        fallback_reason = ?managed_error.detail,
        "[runtime-bootstrap] managed catalog unavailable; using pinned fallback"
    );
    match fallback::install(data_dir, tool_id, task_id, emitter).await {
        Ok(result) => RuntimeComponentReport {
            status: RuntimeComponentStatus::Installed,
            detail: Some(format!("{tool_id} {} (fallback)", result.version)),
        },
        Err(error) => RuntimeComponentReport {
            status: RuntimeComponentStatus::Failed,
            detail: Some(error),
        },
    }
}

fn failed_managed_report(tool_id: &str, error: AppCommandError) -> RuntimeComponentReport {
    tracing::error!(
        tool_id,
        error_code = ?error.code,
        detail = ?error.detail,
        message = %error.message,
        "[runtime-bootstrap] managed runtime install failed without fallback"
    );
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Failed,
        detail: Some(error.message),
    }
}
