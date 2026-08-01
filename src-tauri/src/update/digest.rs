//! App 更新前后持久区摘要记录与对比（IR-006）。
//!
//! 更新前把 `digest_managed_root` 摘要原子写入 `inventory/update-before-digest.json`；
//! 更新后的首次启动调用 `verify_after_update` 对比并清除记录（一次性）。
//! 远端 E2E 断言持久区摘要不变：更新只替换 `app`，不触碰受管组件/用户数据。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::acp::version_center::digest_managed_root;
use crate::app_error::AppCommandError;

const BEFORE_UPDATE_DIGEST_FILE: &str = "update-before-digest.json";

/// 更新前记录的摘要快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeUpdateDigest {
    pub recorded_at: String,
    pub digest: String,
}

/// 更新前后摘要对比结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestComparison {
    pub before: String,
    pub after: String,
    pub unchanged: bool,
}

pub fn before_update_digest_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("inventory").join(BEFORE_UPDATE_DIGEST_FILE)
}

/// 更新前记录持久区摘要（原子写入）。返回记录的摘要。
pub async fn record_before_update(data_dir: &Path) -> Result<String, AppCommandError> {
    let digest = digest_managed_root(data_dir).await?;
    let path = before_update_digest_path(data_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppCommandError::io)?;
    }
    let raw = serde_json::to_vec_pretty(&BeforeUpdateDigest {
        recorded_at: chrono::Utc::now().to_rfc3339(),
        digest: digest.clone(),
    })
    .map_err(|error| {
        AppCommandError::configuration_invalid("Before-update digest is not serializable")
            .with_detail(error.to_string())
    })?;
    let temporary = path.with_extension("json.next");
    tokio::fs::write(&temporary, &raw)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::rename(&temporary, &path)
        .await
        .map_err(AppCommandError::io)?;
    Ok(digest)
}

/// 更新后的首次启动对比并清除记录；无记录时返回 `None`（未发生受管更新）。
///
/// 无论结果如何都移除记录，避免每次启动重复告警。
pub async fn verify_after_update(
    data_dir: &Path,
) -> Result<Option<DigestComparison>, AppCommandError> {
    let path = before_update_digest_path(data_dir);
    let raw = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppCommandError::io(error)),
    };
    let before: BeforeUpdateDigest = serde_json::from_str(&raw).map_err(|error| {
        AppCommandError::configuration_invalid("Before-update digest is corrupted")
            .with_detail(error.to_string())
    })?;
    let after = digest_managed_root(data_dir).await?;
    let unchanged = before.digest == after;
    let comparison = DigestComparison {
        before: before.digest,
        after: after.clone(),
        unchanged,
    };
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AppCommandError::io(error)),
    }
    Ok(Some(comparison))
}
