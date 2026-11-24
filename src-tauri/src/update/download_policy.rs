//! 后台控制制品业务大小，客户端只限制异常元数据和下载等待时间。
use std::time::Duration;

use super::release::ReleaseExtensions;

pub(super) const HARD_MAX_BYTES: u64 = 1024 * 1024 * 1024;
const LEGACY_INSTALLER_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const TOTAL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const READ_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) fn artifact_limit(configured: Option<u64>) -> Result<u64, String> {
    match configured {
        Some(0) => Err("Invalid release download limit".into()),
        Some(value) => Ok(value.min(HARD_MAX_BYTES)),
        None => Ok(HARD_MAX_BYTES),
    }
}

pub(super) fn installer_limit(configured: Option<u64>) -> Result<u64, String> {
    artifact_limit(configured.or(Some(LEGACY_INSTALLER_BYTES)))
}

pub(super) fn validate_size(size: u64, limit: u64) -> Result<(), String> {
    if size == 0 || size > limit {
        return Err(format!(
            "Update download size {size} exceeds the allowed range 1..={limit} bytes"
        ));
    }
    Ok(())
}

pub(super) fn validate_release(ext: &ReleaseExtensions) -> Result<(), String> {
    let limit = artifact_limit(ext.max_artifact_bytes)?;
    if let Some(size) = ext.size {
        validate_size(size, limit)?;
    }
    if let Some(payload) = &ext.app_payload {
        validate_size(payload.size, limit)?;
    }
    if let Some(installer) = &ext.bootstrapper {
        validate_size(installer.size, installer_limit(ext.max_artifact_bytes)?)?;
    }
    Ok(())
}

pub(super) async fn download(
    update: &tauri_plugin_updater::Update,
    progress: &super::state::ProgressEmitter,
) -> Result<Vec<u8>, String> {
    let ext = super::release::extensions(update).map_err(|error| error.to_string())?;
    validate_release(&ext)?;
    let limit = artifact_limit(ext.max_artifact_bytes)?;
    let expected = ext.size.unwrap_or(limit).min(limit);
    let (reject, rejected) = tokio::sync::oneshot::channel();
    let mut reject = Some(reject);
    let mut received = 0_u64;
    let download = update.download(
        |count, length| {
            received = received.saturating_add(count as u64);
            if received > expected || length.is_some_and(|value| value > expected) {
                if let Some(reject) = reject.take() {
                    let _ = reject.send(());
                }
            } else {
                progress.downloading(received, length);
            }
        },
        || progress.verifying(),
    );
    tokio::select! {
        biased;
        _ = rejected => Err("Update download exceeds the declared size or configured limit".into()),
        result = download => {
            let bytes = result.map_err(|error| format!("Update download failed: {error}"))?;
            validate_size(bytes.len() as u64, expected)?;
            Ok(bytes)
        }
    }
}
