use std::path::Path;

use sea_orm::DatabaseConnection;

use crate::acp::version_center::{install_managed_tool, managed_tool_executable};
use crate::app_error::{AppCommandError, AppErrorCode};
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
    if !cfg!(windows) {
        return RuntimeComponentReport {
            status: RuntimeComponentStatus::Skipped,
            detail: Some("managed runtime bootstrap is currently Windows-only".to_string()),
        };
    }
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
        Ok(result) if result.deferred => {
            deferred_report(tool_id, &result.version, task_id, emitter)
        }
        Ok(result) => installed_report(tool_id, &result.version, task_id, emitter),
        Err(error) if fallback_allowed(&error) => {
            install_fallback(data_dir, tool_id, task_id, emitter, &error).await
        }
        Err(error) => managed_failure(tool_id, task_id, emitter, error),
    }
}

fn fallback_allowed(error: &AppCommandError) -> bool {
    if matches!(
        error.code,
        AppErrorCode::NetworkError | AppErrorCode::AuthenticationFailed
    ) {
        return true;
    }
    matches!(
        error.detail.as_deref(),
        Some(
            "AGENT_TOOL_NOT_FOUND"
                | "AGENT_TOOL_POLICY_MISSING"
                | "AGENT_TOOL_DISABLED"
                | "AGENT_TOOL_VERSION_NOT_FOUND"
                | "AGENT_TOOL_ARTIFACT_NOT_READY"
                | "AGENT_STORAGE_UNAVAILABLE"
                | "AGENT_DOWNLOAD_UNAVAILABLE"
                | "AGENT_RATE_LIMITED"
        )
    )
}

fn managed_failure(
    tool_id: &str,
    task_id: &str,
    emitter: &EventEmitter,
    error: AppCommandError,
) -> RuntimeComponentReport {
    let detail = error
        .detail
        .map(|value| format!("{}: {value}", error.message))
        .unwrap_or(error.message);
    tracing::error!(tool_id, error = %detail, "[runtime-bootstrap] managed runtime failed");
    emit(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Failed,
        Some(tool_id.to_string()),
        None,
        detail.clone(),
    );
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Failed,
        detail: Some(detail),
    }
}

fn ready_component(tool_id: &str) -> Option<RuntimeComponentReport> {
    managed_tool_executable(tool_id).map(|path| RuntimeComponentReport {
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

fn deferred_report(
    tool_id: &str,
    version: &str,
    task_id: &str,
    emitter: &EventEmitter,
) -> RuntimeComponentReport {
    let detail = format!(
        "{tool_id} {version} downloaded but activation is deferred until active sessions stop"
    );
    emit(
        emitter,
        task_id,
        RuntimeBootstrapEventKind::Log,
        Some(tool_id.to_string()),
        None,
        &detail,
    );
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Deferred,
        detail: Some(detail),
    }
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
        managed_message = %managed_error.message,
        fallback_reason = ?managed_error.detail,
        "[runtime-bootstrap] managed runtime unavailable; using pinned fallback"
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
