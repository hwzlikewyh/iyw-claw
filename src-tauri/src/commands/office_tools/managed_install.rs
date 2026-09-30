use std::time::Instant;

use super::{
    emit_officecli_install_event, officecli_detect, OfficeToolsError, OfficecliInfo,
    OfficecliInstallEventKind, OFFICECLI_MINIMUM_VERSION,
};
use crate::managed_environment;
use crate::web::event_bridge::EventEmitter;

pub(super) async fn install(
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<OfficecliInfo, OfficeToolsError> {
    let started = Instant::now();
    tracing::info!(task_id, "managed OfficeCLI installation requested");
    emit_officecli_install_event(
        emitter,
        task_id,
        OfficecliInstallEventKind::Log,
        "Waiting for environment maintenance...",
    );
    let result = ensure_officecli(task_id, emitter).await;
    let (kind, message) = match &result {
        Ok(info) => {
            tracing::info!(
                task_id,
                version = ?info.version,
                duration_ms = started.elapsed().as_millis() as u64,
                "managed OfficeCLI installation completed"
            );
            (
                OfficecliInstallEventKind::Completed,
                format!(
                    "OfficeCLI {} is ready",
                    info.version.as_deref().unwrap_or("unknown")
                ),
            )
        }
        Err(error) => {
            tracing::error!(task_id, error = %error, "managed OfficeCLI installation failed");
            (OfficecliInstallEventKind::Failed, error.to_string())
        }
    };
    emit_officecli_install_event(emitter, task_id, kind, message);
    result
}

async fn ensure_officecli(
    task_id: &str,
    emitter: &EventEmitter,
) -> Result<OfficecliInfo, OfficeToolsError> {
    let _guard = managed_environment::lock_writer().await;
    let info = officecli_detect().await;
    if info.installed && info.runtime_error.is_none() && info.compatible {
        tracing::info!(task_id, "managed OfficeCLI already ready after writer wait");
        return Ok(info);
    }
    tracing::info!(
        task_id,
        installed = info.installed,
        compatible = info.compatible,
        runtime_error = ?info.runtime_error,
        "repairing environment for OfficeCLI"
    );
    managed_environment::repair_with_progress(task_id, emitter, |message| {
        emit_officecli_install_event(emitter, task_id, OfficecliInstallEventKind::Log, message);
    })
    .await
    .map_err(OfficeToolsError::CommandFailed)?;
    validate_officecli(officecli_detect().await)
}

fn validate_officecli(info: OfficecliInfo) -> Result<OfficecliInfo, OfficeToolsError> {
    let error = if !info.installed {
        Some("环境修复后仍未找到 OfficeCLI，当前版本的环境计划可能未包含该组件或安装失败被跳过，请检查环境安装记录。".to_string())
    } else if let Some(error) = &info.runtime_error {
        Some(error.clone())
    } else if !info.compatible {
        Some(format!(
            "环境修复后的 OfficeCLI {} 未达到最低兼容版本 {OFFICECLI_MINIMUM_VERSION}，请检查当前版本的环境组件配置。",
            info.version.as_deref().unwrap_or("unknown")
        ))
    } else {
        None
    };
    match error {
        Some(error) => Err(OfficeToolsError::CommandFailed(error)),
        None => Ok(info),
    }
}
