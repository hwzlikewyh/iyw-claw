use std::path::Path;
use std::time::Instant;

use sea_orm::DatabaseConnection;

use crate::acp::version_center::{install_managed_tool, managed_tool_executable};
use crate::app_error::{AppCommandError, AppErrorCode};
use crate::web::event_bridge::EventEmitter;

use super::fallback;
use super::types::{
    emit, RuntimeBootstrapEventKind, RuntimeComponentReport, RuntimeComponentStatus,
};

pub(super) async fn load_channel(conn: &DatabaseConnection, task_id: &str) -> String {
    crate::update::preferences::load(conn)
        .await
        .map(|prefs| prefs.channel.as_str().to_string())
        .unwrap_or_else(|error| {
            tracing::warn!(
                task_id,
                error_code = ?error.code,
                fallback_channel = "stable",
                "failed to load runtime channel; using stable"
            );
            "stable".to_string()
        })
}

pub(super) fn probe_component(tool_id: &str) -> RuntimeComponentReport {
    if let Some(path) = managed_tool_executable(tool_id) {
        return RuntimeComponentReport {
            status: RuntimeComponentStatus::Ready,
            detail: Some(path.to_string_lossy().into_owned()),
        };
    }
    RuntimeComponentReport {
        status: RuntimeComponentStatus::Failed,
        detail: Some(format!(
            "受管 {tool_id} 尚未安装：请先完成桌面初始化（托管分发）"
        )),
    }
}

#[tracing::instrument(
    name = "runtime_bootstrap_component",
    skip_all,
    fields(
        task_id = %task_id,
        tool_id = %tool_id,
        channel = %channel,
        defer_while_active = defer_while_active
    )
)]
pub(super) async fn ensure_component(
    conn: &DatabaseConnection,
    data_dir: &Path,
    tool_id: &str,
    channel: &str,
    defer_while_active: bool,
    task_id: &str,
    emitter: &EventEmitter,
) -> RuntimeComponentReport {
    let started = Instant::now();
    tracing::info!(phase = "begin", "runtime component bootstrap started");
    let report = if let Some(report) = ready_component(tool_id) {
        tracing::info!(outcome = "ready", decision = "already_installed");
        report
    } else {
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
                tracing::info!(
                    outcome = "deferred",
                    decision = "managed_install",
                    version = %result.version
                );
                deferred_report(tool_id, &result.version, task_id, emitter)
            }
            Ok(result) => {
                tracing::info!(
                    outcome = "installed",
                    decision = "managed_install",
                    version = %result.version
                );
                installed_report(tool_id, &result.version, task_id, emitter)
            }
            Err(error) => {
                let allowed = fallback_allowed(&error);
                tracing::warn!(
                    decision = "managed_install_failed",
                    managed_error_code = ?error.code,
                    managed_detail_present = error.detail.is_some(),
                    fallback_allowed = allowed,
                    "managed runtime install decision"
                );
                if allowed {
                    install_fallback(data_dir, tool_id, task_id, emitter, &error).await
                } else {
                    managed_failure(tool_id, task_id, emitter, error)
                }
            }
        }
    };
    tracing::info!(
        phase = "end",
        outcome = ?report.status,
        duration_ms = started.elapsed().as_millis() as u64,
        "runtime component bootstrap finished"
    );
    report
}

fn fallback_allowed(error: &AppCommandError) -> bool {
    if error.code == AppErrorCode::NetworkError {
        return true;
    }
    if error.code != AppErrorCode::InvalidInput {
        return false;
    }
    matches!(
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
    )
}

fn managed_failure(
    tool_id: &str,
    task_id: &str,
    emitter: &EventEmitter,
    error: AppCommandError,
) -> RuntimeComponentReport {
    tracing::error!(
        tool_id,
        task_id,
        error_code = ?error.code,
        detail_present = error.detail.is_some(),
        error_message = %error.message,
        "[runtime-bootstrap] managed runtime failed"
    );
    let detail = error
        .detail
        .map(|value| format!("{}: {value}", error.message))
        .unwrap_or(error.message);
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
    let started = Instant::now();
    tracing::warn!(
        tool_id,
        task_id,
        managed_error_code = ?managed_error.code,
        managed_detail_present = managed_error.detail.is_some(),
        "[runtime-bootstrap] managed runtime unavailable; using pinned fallback"
    );
    let report = match fallback::install(data_dir, tool_id, task_id, emitter).await {
        Ok(result) => {
            tracing::info!(
                tool_id,
                task_id,
                outcome = "installed",
                source = "pinned_fallback",
                version = result.version,
                duration_ms = started.elapsed().as_millis() as u64,
                "runtime fallback finished"
            );
            RuntimeComponentReport {
                status: RuntimeComponentStatus::Installed,
                detail: Some(format!("{tool_id} {} (fallback)", result.version)),
            }
        }
        Err(error) => {
            tracing::error!(
                tool_id,
                task_id,
                outcome = "failed",
                source = "pinned_fallback",
                error_detail_present = true,
                duration_ms = started.elapsed().as_millis() as u64,
                "runtime fallback failed"
            );
            RuntimeComponentReport {
                status: RuntimeComponentStatus::Failed,
                detail: Some(error),
            }
        }
    };
    report
}
